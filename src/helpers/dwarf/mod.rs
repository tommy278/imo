pub mod debug_frame;
pub mod debug_info;
pub mod debug_line;
pub mod error;

use crate::helpers::dwarf::debug_info::Abi;
use crate::interface::RegisterViewer;

/// Evaluate the frame base from the raw bytes by reading them as a stream of byte code
pub fn evaluate_frame_base_bytes(bytes: &[u8], registers: &RegisterViewer, abi: &Abi) -> u64 {
    let mut stack: Vec<u64> = Vec::new();
    let mut i = 0;

    while i < bytes.len() {
        let op = bytes[i];
        i += 1;

        match op {
            // DW_OP_reg0 (0x50) to DW_OP_reg31 (0x6F)
            0x50..=0x6F => {
                let dw_reg_num = (op - 0x50) as u16;
                let value = abi.get_register_value(dw_reg_num, registers);
                stack.push(value);
            }

            // DW_OP_plus (0x22) - Adds top two stack values
            0x22 => {
                let b = stack.pop().unwrap();
                let a = stack.pop().unwrap();
                stack.push(a.wrapping_add(b));
            }

            // DW_OP_minus (0x1c) - Subtracts top from second
            0x1C => {
                let b = stack.pop().unwrap();
                let a = stack.pop().unwrap();
                stack.push(a.wrapping_sub(b));
            }

            _ => panic!("Unsupported opcode: 0x{:x}", op),
        }
    }

    stack.pop().unwrap()
}
