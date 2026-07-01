// NOTE: Compile with rustc -g rust_with_vars.rs

fn main() {
    // Skip char support for now
    let c: char = 'l';
    let f: f32 = 3.141;
    let x: i64 = 10000000;
    let y: i64 = -15;
    let z: u16 = 256;
    let opt = Some(12);
    let b = false;
    let t = true;
    let arr: [i32; 6] = [120, -1, 26, 34, -5000, 720];
    let small_arr: [u8; 4] = [10, 18, 19, 20];
    let matrix = [[1, 2, 3, 4], [5, 6, 7, 8], [9, 10, 11, 12]];
    let ptr = std::ptr::addr_of!(x);
    println!("{:p}", ptr);

    // NOTE: Debugger does not recognize these types yet
    let vec = vec![1, 2, 3, 4, 5];
    println!("{}", x + y);
}
