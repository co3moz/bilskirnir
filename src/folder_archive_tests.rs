//! A folder's download: which paths download at all, and the archive's file name.
use super::*;

#[test]
fn files_download_as_they_are_and_folders_as_archives() {
    let file = json!({"name": "notes.txt", "type": "file", "size": 42});
    assert_eq!(target(&file).unwrap(), Target::File(42));
    let folder = json!({"name": "Documents", "type": "directory", "size": 4096});
    assert_eq!(target(&folder).unwrap(), Target::Folder);
}

#[test]
fn links_and_special_files_are_refused() {
    for kind in ["symlink", "special"] {
        let refused = target(&json!({"name": "x", "type": kind, "size": 0})).unwrap_err();
        assert_eq!(refused.0, StatusCode::BAD_REQUEST, "{kind}");
    }
}

#[test]
fn the_archive_is_named_after_its_folder() {
    assert_eq!(
        disposition("Documents"),
        "attachment; filename=\"Documents.tar.gz\"; filename*=UTF-8''Documents.tar.gz"
    );
    // A name a quoted string cannot carry keeps only safe characters there, and all of itself
    // in `filename*`.
    assert_eq!(
        disposition("Masaüstü \"x\";y"),
        "attachment; filename=\"Masa_st_ _x__y.tar.gz\"; \
         filename*=UTF-8''Masa%C3%BCst%C3%BC%20%22x%22%3By.tar.gz"
    );
}
