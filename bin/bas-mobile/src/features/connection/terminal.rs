use bas_client::acp;
use nsh_base::{TerminalEmulator, TerminalSize, VtEmulator};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

const COLUMNS: u16 = 160;
const ROWS: u16 = 48;
const SCROLLBACK: usize = 1000;

#[derive(Clone)]
pub(crate) struct Terminal {
    parser: Rc<RefCell<VtEmulator>>,
    pub output: String,
    pub cwd: Option<String>,
    pub exit_code: Option<u64>,
    pub signal: Option<String>,
}
impl Default for Terminal {
    fn default() -> Self {
        Self {
            parser: Rc::new(RefCell::new(VtEmulator::new(
                TerminalSize::new(COLUMNS, ROWS),
                SCROLLBACK,
            ))),
            output: String::new(),
            cwd: None,
            exit_code: None,
            signal: None,
        }
    }
}

// ACP's display-only terminal extension, also consumed by GDS Agents.
pub(super) fn apply(terminals: &mut BTreeMap<String, Terminal>, meta: Option<&acp::Meta>) {
    let Some(meta) = meta else { return };
    for key in ["terminal_info", "terminal_output", "terminal_exit"] {
        let Some(value) = meta.get(key) else { continue };
        let Some(id) = value.get("terminal_id").and_then(|v| v.as_str()) else {
            continue;
        };
        let terminal = terminals.entry(id.into()).or_default();
        match key {
            "terminal_info" => {
                terminal.cwd = value.get("cwd").and_then(|v| v.as_str()).map(str::to_owned)
            }
            "terminal_output" => {
                if let Some(data) = value.get("data").and_then(|v| v.as_str()) {
                    let mut parser = terminal.parser.borrow_mut();
                    parser.feed(data.replace('\n', "\r\n").as_bytes());
                    terminal.output = (0..parser.line_count())
                        .map(|i| parser.line(i).text().trim_end().to_owned())
                        .collect::<Vec<_>>()
                        .join("\n")
                        .trim_end()
                        .to_owned();
                }
            }
            "terminal_exit" => {
                terminal.exit_code = value.get("exit_code").and_then(|v| v.as_u64());
                terminal.signal = value
                    .get("signal")
                    .and_then(|v| v.as_str())
                    .map(str::to_owned);
            }
            _ => {}
        }
    }
}
