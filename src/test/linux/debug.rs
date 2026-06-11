use crate::linux;

#[test]
fn debug() {
    let current_dir = std::env::current_dir().unwrap();

    let mut dir = current_dir.display().to_string();
    dir.push_str("/src/test/linux/running_task/running_task");

    linux::debug(&dir);
}
