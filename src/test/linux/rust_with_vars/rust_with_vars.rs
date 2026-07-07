// NOTE: Compile with rustc -g rust_with_vars.rs

fn main() {
    // Skip char support for now
    let c: char = 'l';
    let f: f32 = 3.141;
    let x: i64 = 10000000;
    let y: i64 = -15;
    let z: u16 = 256;

    let u8_some: Option<u8> = Some(8);
    let u16_some: Option<u16> = Some(16);
    let u32_some: Option<u32> = Some(32);
    let u64_some: Option<u64> = Some(64);
    let non: Option<i32> = None;

    let b = false;
    let t = true;
    let arr: [i32; 6] = [120, -1, 26, 34, -5000, 720];
    let small_arr: [u8; 4] = [10, 18, 19, 20];
    let matrix = [[1, 2, 3, 4], [5, 6, 7, 8], [9, 10, 11, 12]];
    let ptr = std::ptr::addr_of!(x);
    let e: Letters = Letters::E;
    let cid = CustomEnum::Id(101);
    let ce: Option<CustomEnum> = Some(CustomEnum::Age(12, true));
    let cn = CustomEnum::Name("Luke".to_string());
    let cs = CustomStruct {
        name: "Tim".to_string(),
        id: 102,
        age: 216,
        en: CustomEnum::Id(10),
    };
    let ok: Result<u8, &str> = Ok(152);
    let err: Result<u8, u16> = Err(256);
    let de: Option<String> = Some(String::from("Hello World"));
    let tup = ("Tuple".to_string(), true);
    let ms = MyOption::Some("Test");
    let mn: MyOption<i32> = MyOption::None;
    let ec = EdgeCaseEnum::LayoutTrap(42, 999_999_999, true);

    // NOTE: Debugger does not recognize these types yet
    let vec = vec![1, 2, 3, 4, 5];
    let box_type = Box::new(12);
    let static_str = "Hello World";
    let string = String::from(static_str);
    let slice = &string[1..];
    println!("{}", x + y);
}

enum Letters {
    A,
    B,
    C,
    D,
    E,
}

pub enum MyOption<T> {
    Some(T),
    None,
}

pub enum CustomEnum {
    Id(u8),
    Name(String),
    Age(u16, bool),
    Location(String),
}

pub struct CustomStruct {
    name: String,
    id: usize,
    age: u8,
    en: CustomEnum,
}

pub enum EdgeCaseEnum {
    LayoutTrap(u8, u64, bool),
}
