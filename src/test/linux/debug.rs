use crate::linux;

#[test]
fn running_task() {
    let current_dir = std::env::current_dir().unwrap();

    let mut dir = current_dir.display().to_string();
    dir.push_str("/src/test/linux/running_task/running_task");

    linux::debug(&dir);
}

#[test]
fn multiple() {
    let current_dir = std::env::current_dir().unwrap();

    let mut dir = current_dir.display().to_string();
    dir.push_str("/src/test/linux/multiple/inline_test");

    linux::debug(&dir);
}
