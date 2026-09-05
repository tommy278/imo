use std::fs::File;
use std::io::Write;

#[cfg(test)]
macro_rules! write_to_output {
    ({{ $([[ $($inner_code:tt)* ]])* }}) => {
        let mut current_path= std::env::current_dir().unwrap().display().to_string();
        let internal_path = "/src/test/out/out.rs";
        current_path.push_str(internal_path);
        let mut file = File::create(current_path).unwrap();

        let mut full_str = String::new();

        $(
            let inner_code_str = stringify!($($inner_code)*);
            full_str.push_str(inner_code_str);
            full_str.push('\n');
        )*

        file.write(b"fn main {\n").unwrap();
        file.write(full_str.as_ref()).unwrap();
        file.write(b"}\n").unwrap();
    };
    ({{ $([[ $($lib_decl:tt)* ]])* }}, {{ $([[ $($inner_code:tt)* ]])* }}) => {
        let mut current_path= std::env::current_dir().unwrap().display().to_string();
        let internal_path = "/src/test/out/out.rs";
        current_path.push_str(internal_path);
        let mut file = File::create(current_path).unwrap();

        let mut full_lib_decl_str = String::new();

        $(
            let lib_decl_str = stringify!($($lib_decl)*);
            full_lib_decl_str.push_str(lib_decl_str);
            full_lib_decl_str.push('\n');
        )*

        let mut full_inner_code = String::new();

        $(
            let inner_code_str = stringify!($($inner_code)*);
            full_inner_code.push_str(inner_code_str);
            full_inner_code.push('\n');
        )*

        file.write(full_lib_decl_str.as_ref()).unwrap();
        file.write(b"fn main {\n").unwrap();
        file.write(full_inner_code.as_ref()).unwrap();
        file.write(b"}\n").unwrap();
    }
}

#[test]
fn hello_world() {
    write_to_output!(
        {{
            [[use std::io::Write; ]]
        }},
        {{
            [[ let name = "Hello World"; ]]
            [[ let x = 14; ]]
         }}
    );
}
