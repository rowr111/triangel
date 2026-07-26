use crate::cmds::ShellCmdApi;
use crate::input::ir;

// Diagnostics for the IR remote receiver. Kept for the future even though
// console input is currently unavailable on this system build.
pub struct IrCmd {}

impl<'a> ShellCmdApi<'a> for IrCmd {
    cmd_api!(ir);

    fn process(&mut self, args: String, _env: &mut super::CommonEnv) -> Result<Option<String>, xous::Error> {
        use core::fmt::Write;
        let mut ret = String::new();
        let helpstring = "ir [status]";

        let mut tokens = args.split(' ');
        match tokens.next() {
            Some("status") => {
                let (clock_hz, decoded, rejected, last_frame, edges) = ir::stats();
                write!(
                    ret,
                    "clock: {} Hz, edges: {}, decoded: {}, rejected: {}, last frame: {:08x}",
                    clock_hz, edges, decoded, rejected, last_frame
                )
                .unwrap();
            }
            _ => {
                write!(ret, "{}", helpstring).unwrap();
            }
        }
        Ok(Some(ret))
    }
}
