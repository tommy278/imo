// NOTE: Compile with rustc -g rust_with_vars.rs

fn main() {
    let f: f32 = 3.141;
    let x: i64 = 10000000;
    let y: i64 = -15;
    let z: u16 = 256;
    let opt = Some(12);
    let b = false;
    let t = true;
    let arr: [i32; 6] = [0, 1, 2, 3, 4, 6];
    let ptr = std::ptr::addr_of!(x);
    println!("{:p}", ptr);

    // NOTE: Debugger does not recognize these types yet
    let vec = vec![1, 2, 3, 4, 5];
    println!("{}", x + y);
}
