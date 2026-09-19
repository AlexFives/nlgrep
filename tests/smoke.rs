use assert_cmd::Command;

#[test]
fn binary_shows_help() {
    let mut command = Command::cargo_bin("nlgrep").expect("binary should be built");
    command.arg("--help").assert().success();
}
