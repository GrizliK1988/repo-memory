use repo_memory::ifds::{discover_fixtures, load_fixture};
use std::path::PathBuf;

#[test]
fn hand_authored_ifds_fixtures_are_discoverable() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/ifds/fixtures");
    let fixtures = discover_fixtures(&root).expect("the K003 fixture suite must not be empty");
    let mut case_ids = fixtures
        .iter()
        .map(|path| {
            load_fixture(path)
                .expect("fixture must load and validate")
                .manifest
                .case_id
        })
        .collect::<Vec<_>>();
    case_ids.sort();
    assert_eq!(
        case_ids,
        ["copy_then_overwrite", "full_downstream_chain", "overwrite"]
    );
}
