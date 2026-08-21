// NOTE: Compile with rustc -g rust_with_vars.rs
use std::collections::HashMap;
fn main() {
    let c: char = 'l';
    let f: f32 = 3.141;
    let mut x: i64 = 10000000;
    let y: i64 = -15;
    let z: u16 = 256;
    let q = 5;

    let u8_some: Option<u8> = Some(8);
    let u16_some: Option<u16> = Some(16);
    let u32_some: Option<u32> = Some(32);
    let u64_some: Option<u64> = Some(64);
    let mut non: Option<i32> = None;
    let mut g: HashMap<u8, &str> = HashMap::new();
    g.insert(1, "foo");
    g.insert(2, "bar");
    g.insert(3, "baz");

    let cell = std::cell::Cell::new(100);
    let rc = std::rc::Rc::new("Rc");
    let arc = std::sync::Arc::new("Arc");

    let mut deq = std::collections::VecDeque::new();
    deq.push_back(5);
    deq.push_front(4);
    deq.push_front(3);
    deq.push_front(2);
    deq.push_back(6);

    let b = false;
    let mut t = true;
    let arr: [i32; 6] = [120, -1, 26, 34, -5000, 720];
    let small_arr: [u8; 4] = [10, 18, 19, 20];
    let matrix = [[1, 2, 3, 4], [5, 6, 7, 8], [9, 10, 11, 12]];
    let ptr = std::ptr::addr_of!(x);
    let e: Letters = Letters::E;
    let cl = Letters::C;
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
    let ok2: Result<String, u8> = Ok("tested".to_string());
    let err2: Result<&str, bool> = Err(false);
    let de: Option<String> = Some(String::from("Hello World"));
    let tup = ("Tuple".to_string(), true);
    let ms = MyOption::Some("Test");
    let mn: MyOption<i32> = MyOption::None;
    let ec = EdgeCaseEnum::LayoutTrap(42, 999_999_999, true);
    let n = Niche::Value("niche".to_string());
    let ne = Niche::Empty;
    let oe: Option<char> = Some('b');
    let nested_some: Option<Option<bool>> = Some(None);
    let nested_none: Option<Option<bool>> = None;
    let status_a = StackedStatus::Active(true);
    let status_b = StackedStatus::Suspended;

    let mut named = || {
        let foo = [1, 2, 6, 24];
        let bar = vec![1, 1, 2, 3, 5, 8, 13, 21, 34, 55, 89, 144];
        let baz = Box::new(15);
        let test = Box::new(19);
        x += 120;
    };
    named();

    if x > 12 {
        let mut names = Vec::new();
        names.push("Halland");
        names.push("Dave");
        names.push("Cleo");
        println!("Hello World");
    }

    add(arr[2], arr[3]);
    let x = fib(20);

    // NOTE: Debugger does not recognize these types yet
    let vec = vec![1, 2, 3, 4, 5];
    let nested_vec = vec![vec![2]];
    let box_type = Box::new(12);
    let static_str = "Hello World";
    let string = String::from(static_str);
    let slice = &string[1..];
}

fn fib(n: u128) -> u128 {
    if n == 0 || n == 1 {
        return n;
    }

    fib(n - 1) + fib(n - 2)
}

fn add(x: i32, y: i32) -> i32 {
    let t = div(x, y);
    x + y
}

fn div(x: i32, y: i32) -> i32 {
    let x = mul(x, y);
    x / y
}

fn mul(x: i32, y: i32) -> i32 {
    let x = sub(x, y);
    x * y
}

fn sub(x: i32, y: i32) -> i32 {
    x - y
}

pub enum StackedStatus {
    Active(bool),
    Pending,
    Suspended,
    Deleted,
}

#[repr(u64)]
enum Letters {
    A = 100,
    B = 200,
    C = 300,
    D = 400,
    E = 500,
}

pub enum MyOption<T> {
    Some(T),
    None,
}

pub enum Niche {
    Value(String),
    Empty,
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
