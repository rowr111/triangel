#!/usr/bin/env python3
"""
ear_sim.py - Desktop audio simulator for the Triangel ear chip.

Captures audio from a microphone or system loopback and streams it to the
ear board over USB serial in the same frame format the ICS43434 MEMS mic
would produce. The ear board firmware (uart-audio feature) reads these frames,
runs the mel filterbank, and sends MelFrames to the eye board over UART -
identical to production mode with the real mic.

Wire format: 0xBB sync byte + 512 x i16 LE samples = 1025 bytes per packet
at ~31 fps (512 samples / 16000 Hz = 32 ms per frame).

Usage:
    python ear_sim.py                          # desktop audio, COM6 (defaults)
    python ear_sim.py --mic                    # use mic instead
    python ear_sim.py --device 2               # specific mic device index
    python ear_sim.py --list-devices

COM6 is the ear board's USB CDC serial port. Change EAR_PORT if it differs.

Dependencies:
    pip install pyaudiowpatch numpy pyserial
"""

import argparse
import math
import queue
import sys
import threading
import time

import numpy as np
import pyaudiowpatch as pyaudio
import serial

# These must match the firmware constants in triangel-shared and audio.rs.
SAMPLE_RATE  = 16_000   # Hz
FRAME_SIZE   = 512      # samples per packet (FFT_SIZE in firmware)
PCM_SYNC     = 0xBB     # sync byte at the start of each packet
UART_BAUD    = 1_000_000  # EAR_UART_BAUD in triangel-shared

PACKET_BYTES = 1 + FRAME_SIZE * 2  # 1025: sync + 512 i16 LE samples

# USB serial port for the ear board. Appears as a CDC serial device when
# connected via USB. Check Device Manager -> Ports if this needs updating.
EAR_PORT = "COM6"


class Stats:
    """Shared state updated by the audio thread, read by the display thread."""
    def __init__(self):
        self._lock = threading.Lock()
        self.rms = 0.0
        self.fps = 0.0
        self._frames = 0
        self._t0 = time.monotonic()

    def update(self, rms: float) -> None:
        now = time.monotonic()
        with self._lock:
            self.rms = rms
            self._frames += 1
            elapsed = now - self._t0
            if elapsed >= 1.0:
                self.fps = self._frames / elapsed
                self._frames = 0
                self._t0 = now

    def snapshot(self) -> tuple:
        with self._lock:
            return self.rms, self.fps


def display_loop(stats: Stats, stop: threading.Event) -> None:
    bar_width = 20
    while not stop.is_set():
        rms, fps = stats.snapshot()
        filled = round(rms * bar_width)
        bar = "█" * filled + "░" * (bar_width - filled)
        pct = round(rms * 100)
        print(f"\r  [{bar}]  {pct:3d}% RMS  |  {fps:4.1f} fps  ", end="", flush=True)
        stop.wait(timeout=0.1)
    print()


def find_loopback_device(p: pyaudio.PyAudio) -> dict:
    """Return the WASAPI loopback device for the system default speakers."""
    try:
        wasapi = p.get_host_api_info_by_type(pyaudio.paWASAPI)
    except OSError:
        sys.exit("Error: WASAPI not available on this system.")
    default_out = p.get_device_info_by_index(wasapi["defaultOutputDevice"])
    for lb in p.get_loopback_device_info_generator():
        if default_out["name"] in lb["name"]:
            return lb
    sys.exit("Error: no loopback device found for the default speakers.")


def list_devices() -> None:
    p = pyaudio.PyAudio()
    print("Mic / input devices:")
    for i in range(p.get_device_count()):
        info = p.get_device_info_by_index(i)
        if info["maxInputChannels"] > 0:
            print(f"  {i:3d}  {info['name']}  ({int(info['defaultSampleRate'])} Hz)")
    print("\nLoopback devices (desktop audio capture):")
    for lb in p.get_loopback_device_info_generator():
        print(f"  {lb['index']:3d}  {lb['name']}  ({int(lb['defaultSampleRate'])} Hz)")
    print(f"\nDefault serial port: {EAR_PORT}")
    p.terminate()


def resample(mono: np.ndarray, from_rate: int, to_rate: int, n_out: int) -> np.ndarray:
    """Downsample mono float32 array from from_rate to to_rate.

    Uses box-filter decimation when the ratio is an exact integer (e.g. 48kHz
    -> 16kHz = factor 3), otherwise falls back to linear interpolation. Both
    approaches are good enough for mel filterbank LED effects.
    """
    if from_rate == to_rate:
        return mono[:n_out]
    factor = from_rate / to_rate
    if from_rate % to_rate == 0:
        # Exact integer ratio: average groups of `factor` samples (anti-aliased)
        f = int(factor)
        trimmed = mono[:len(mono) - len(mono) % f]
        return trimmed.reshape(-1, f).mean(axis=1)[:n_out]
    else:
        # Fractional ratio: linear interpolation
        x_out = np.linspace(0, len(mono) - 1, n_out)
        return np.interp(x_out, np.arange(len(mono)), mono)


def capture_loopback(p: pyaudio.PyAudio, frame_queue: "queue.Queue[bytes]", stats: Stats, device_index=None) -> None:
    if device_index is not None:
        device = p.get_device_info_by_index(device_index)
    else:
        device = find_loopback_device(p)
    native_rate = int(device["defaultSampleRate"])
    channels    = device["maxInputChannels"]

    # Read enough native frames to produce at least FRAME_SIZE output samples
    native_frames = math.ceil(FRAME_SIZE * native_rate / SAMPLE_RATE)

    print(f"  Device : {device['name']}")
    print(f"  Rate   : {native_rate} Hz -> {SAMPLE_RATE} Hz")

    stream = p.open(
        format=pyaudio.paInt16, channels=channels, rate=native_rate,
        frames_per_buffer=native_frames, input=True,
        input_device_index=device["index"],
    )
    try:
        while True:
            data    = stream.read(native_frames, exception_on_overflow=False)
            raw     = np.frombuffer(data, dtype=np.int16).reshape(-1, channels)
            mono    = raw.mean(axis=1).astype(np.float32) / 32768.0
            out     = resample(mono, native_rate, SAMPLE_RATE, FRAME_SIZE)
            rms     = float(np.sqrt(np.mean(out ** 2)))
            stats.update(min(rms * 10.0, 1.0))  # scale up: speech/music RMS ~0.05-0.1
            samples = (out * 32767.0).astype(np.int16)
            try:
                frame_queue.put_nowait(bytes([PCM_SYNC]) + samples.tobytes())
            except queue.Full:
                print("[warn] dropping frame - serial too slow", file=sys.stderr)
    finally:
        stream.stop_stream()
        stream.close()


def capture_mic(p: pyaudio.PyAudio, device, frame_queue: "queue.Queue[bytes]", stats: Stats) -> None:
    if device is not None:
        name = p.get_device_info_by_index(device)["name"]
    else:
        name = "default mic"
    print(f"  Device : {name}")

    stream = p.open(
        format=pyaudio.paInt16, channels=1, rate=SAMPLE_RATE,
        frames_per_buffer=FRAME_SIZE, input=True,
        input_device_index=device,
    )
    try:
        while True:
            data    = stream.read(FRAME_SIZE, exception_on_overflow=False)
            pcm     = np.frombuffer(data, dtype=np.int16).astype(np.float32) / 32768.0
            rms     = float(np.sqrt(np.mean(pcm ** 2)))
            stats.update(min(rms * 10.0, 1.0))
            try:
                frame_queue.put_nowait(bytes([PCM_SYNC]) + data)
            except queue.Full:
                print("[warn] dropping frame - serial too slow", file=sys.stderr)
    finally:
        stream.stop_stream()
        stream.close()


def run(port: str, baud: int, device, loopback: bool) -> None:
    frame_queue: "queue.Queue[bytes | None]" = queue.Queue(maxsize=4)
    reconnect_delay = 2.0

    ser_lock = threading.Lock()
    ser_ref: list = [None]  # mutable serial reference shared with writer thread

    def open_serial() -> bool:
        try:
            s = serial.Serial(port, baud, timeout=1)
            with ser_lock:
                ser_ref[0] = s
            print(f"\rSerial open: {port} @ {baud} baud          ")
            return True
        except serial.SerialException as e:
            print(f"\rSerial unavailable ({e}), retrying in {reconnect_delay:.0f}s...", end="")
            return False

    # Wait for initial connection
    while not open_serial():
        time.sleep(reconnect_delay)

    def serial_writer() -> None:
        while True:
            packet = frame_queue.get()
            if packet is None:
                return
            while True:
                with ser_lock:
                    s = ser_ref[0]
                if s is None:
                    time.sleep(0.1)
                    continue
                try:
                    s.write(packet)
                    break
                except serial.SerialException:
                    with ser_lock:
                        try: ser_ref[0].close()
                        except Exception: pass
                        ser_ref[0] = None
                    print("\rSerial lost, reconnecting...", end="")
                    while not open_serial():
                        time.sleep(reconnect_delay)
                    break  # drop this packet, resume with next

    writer = threading.Thread(target=serial_writer, daemon=True)
    writer.start()

    mode = "loopback" if loopback else "mic"
    fps  = SAMPLE_RATE / FRAME_SIZE
    print(f"Streaming {mode} -> {port} @ {baud} baud, ~{fps:.0f} fps")
    print(f"  Packet : {FRAME_SIZE} samples x i16 + sync = {PACKET_BYTES} bytes")

    stats     = Stats()
    stop_disp = threading.Event()

    p = pyaudio.PyAudio()
    if loopback:
        target, args = capture_loopback, (p, frame_queue, stats, device if device is not None else None)
    else:
        target, args = capture_mic, (p, device, frame_queue, stats)

    audio   = threading.Thread(target=target, args=args, daemon=True)
    display = threading.Thread(target=display_loop, args=(stats, stop_disp), daemon=True)
    audio.start()

    print("Press Ctrl+C to stop.")
    display.start()
    try:
        audio.join()
    except KeyboardInterrupt:
        pass
    finally:
        stop_disp.set()
        display.join(timeout=1)
        frame_queue.put(None)
        writer.join(timeout=2)
        with ser_lock:
            if ser_ref[0]: ser_ref[0].close()
        p.terminate()
        print("Stopped.")


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Stream desktop audio to the Triangel ear board over USB serial.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=__doc__,
    )
    parser.add_argument("--port", default=EAR_PORT,
        help=f"Ear board USB CDC serial port (default: {EAR_PORT})")
    parser.add_argument("--baud", type=int, default=UART_BAUD,
        help=f"Baud rate (default: {UART_BAUD} - must match firmware)")
    parser.add_argument("--device", type=int, default=None,
        help="Mic device index (--mic mode only). Use --list-devices to see options.")
    parser.add_argument("--loopback", action="store_true", default=True,
        help="Capture desktop audio via WASAPI loopback (default).")
    parser.add_argument("--mic", dest="loopback", action="store_false",
        help="Use microphone input instead of loopback.")
    parser.add_argument("--list-devices", action="store_true",
        help="List available audio and loopback devices and exit.")
    args = parser.parse_args()

    if args.list_devices:
        list_devices()
        return

    run(args.port, args.baud, args.device, args.loopback)


if __name__ == "__main__":
    main()
