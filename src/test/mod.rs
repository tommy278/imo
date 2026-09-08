use std::fs::File;
use std::io::{BufRead, Write};

#[cfg(test)]
macro_rules! write_to_output {
    ({{ $([[ $($inner_code:tt)* ]])* }}) => {
        let mut current_path= std::env::current_dir().unwrap().display().to_string();
        let internal_path = "/src/test/out/out.rs";
        current_path.push_str(internal_path);

        {
           std::fs::create_dir_all(current_path.strip_suffix("/out.rs").unwrap());
           std::fs::File::create(current_path.strip_suffix(".rs").unwrap());
        }

        let mut file = File::create(&current_path).unwrap();

        let mut full_str = String::new();

        $(
            let inner_code_str = stringify!($($inner_code)*);
            full_str.push_str(inner_code_str);
            full_str.push('\n');
        )*

        file.write(b"fn main() {\n").unwrap();
        file.write(full_str.as_ref()).unwrap();
        file.write(b"}\n").unwrap();
    };
    ({{ $([[ $($lib_decl:tt)* ]])* }}, {{ $([[ $($inner_code:tt)* ]])* }}) => {
        let mut current_path= std::env::current_dir().unwrap().display().to_string();
        let internal_path = "/src/test/out/out.rs";
        current_path.push_str(internal_path);

        {
           std::fs::create_dir_all(current_path.strip_suffix("/out.rs"));
           std::fs::File::create(current_path.strip_suffix(".rs"));
        }

        let mut file = File::create(&current_path).unwrap();

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
        file.write(b"fn main() {\n").unwrap();
        file.write(full_inner_code.as_ref()).unwrap();
        file.write(b"}\n").unwrap();

    }
}

#[cfg(test)]
fn compile(source_path: &str, binary_path: &str) {
    std::process::Command::new("rustc")
        .arg("-g")
        .arg("-o")
        .arg(binary_path)
        .arg(source_path)
        .output()
        .expect("Failed to compile code");
}

#[cfg(test)]
fn create_process() -> std::process::Child {
    let mut binary = std::env::current_dir().unwrap().display().to_string();
    let internal_path = "/src/test/out/out";
    binary.push_str(internal_path);
    {
        let mut source_path = binary.clone();
        source_path.push_str(".rs");
        compile(&source_path, &binary);
    }

    let child = std::process::Command::new("cargo")
        .arg("run")
        .arg(binary)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("Failed to start debugger");
    child
}

#[cfg(test)]
fn write_and_read(child: &mut std::process::Child, cmd: &str) -> String {
    let mut stdin = child.stdin.take().expect("Failed to open stdin");
    let stdout = child.stdout.take().expect("Failed to open stdout");
    let mut reader = std::io::BufReader::new(stdout);

    writeln!(stdin, "{cmd}").expect("Failed to write to stdin");
    stdin.flush().unwrap();

    let mut response = String::new();
    reader
        .read_line(&mut response)
        .expect("Failed to read line");

    child.stdin = Some(stdin);
    child.stdout = Some(reader.into_inner());

    response
}

#[cfg(test)]
fn get_val(dbg_val: &str) -> &str {
    let (_, val) = dbg_val.split_once('=').unwrap();
    &val[1..]
}

#[cfg(test)]
macro_rules! cmp {
    ($first:expr, $second:expr) => {
        let dbg_val = get_val($first);
        assert!(dbg_val.contains($second));
    };
}

#[test]
fn integer() {
    write_to_output!(
        {{
            [[ let x:i32 = -15; ]]
            [[ let p: i8 = -2; ]]
            [[ let d: u64 = 12; ]]
            [[ let e: usize = 13; ]]
            [[ let _n = 500; ]] // Placeholder for a breakpoint to be placed
         }}
    );
    let mut child = create_process();
    let _ = write_and_read(&mut child, "b 6");

    let _ = write_and_read(&mut child, "run");

    let x = write_and_read(&mut child, "p x");
    cmp!(&x, "-15");

    let p = write_and_read(&mut child, "p p");
    cmp!(&p, "-2");

    let d = write_and_read(&mut child, "p d");
    cmp!(&d, "12");

    let e = write_and_read(&mut child, "p e");
    cmp!(&e, "13");

    child.kill().unwrap();
}
