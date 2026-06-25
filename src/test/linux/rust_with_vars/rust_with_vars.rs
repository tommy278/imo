// NOTE: Compile with rustc -g rust_with_vars.rs

fn main() {
    let x: i64 = 10;
    let y: i64 = 12;
    let opt = Some(12);
    let arr: [i32; 5] = [0, 1, 2, 3, 4];

    // NOTE: Debugger does not recognize these types yet
    let vec = vec![1, 2, 3, 4, 5];
    let ptr = Box::new(12);
    println!("{}", x + y);
}
