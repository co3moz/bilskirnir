//! Pins down which file tool requests are refused, among them a recursive delete of the home or
//! of a volume's mount point, also one a compose mounts.
use super::*;
use crate::model;
use crate::test_support::spec;

#[test]
fn file_tool_requests_are_validated() {
    let mut spec = spec();
    spec.mounts.push(model::Mount {
        volume: "projects".into(),
        target: "/shared".into(),
        readonly: false,
    });
    assert!(
        ListFiles {
            path: "/".into(),
            limit: 10
        }
        .validate()
        .is_ok()
    );
    assert!(
        ListFiles {
            path: "relative".into(),
            limit: 10
        }
        .validate()
        .is_err()
    );
    assert!(
        ListFiles {
            path: "/home/desktop".into(),
            limit: 0
        }
        .validate()
        .is_err()
    );
    assert!(
        MoveFile {
            source: "/home/desktop/a".into(),
            destination: "/home/desktop/b".into(),
            replace: false
        }
        .validate()
        .is_ok()
    );
    assert!(
        MoveFile {
            source: "/home/desktop/../a".into(),
            destination: "/home/desktop/b".into(),
            replace: false
        }
        .validate()
        .is_err()
    );
    for path in ["/home/desktop", "/shared", "/etc", "/home/desktop/"] {
        assert!(
            DeleteFile {
                path: path.into(),
                recursive: true
            }
            .validate(&spec.mounts)
            .is_err(),
            "{path}"
        );
    }
    assert!(
        DeleteFile {
            path: "/shared/project".into(),
            recursive: true
        }
        .validate(&spec.mounts)
        .is_ok()
    );
    // A volume a compose parameter mounts is a root too, though the settings never list it.
    let composed = model::Mount {
        volume: "thoric-c-DATA".into(),
        target: "/home/desktop/Projects".into(),
        readonly: false,
    };
    let data = DeleteFile {
        path: "/home/desktop/Projects".into(),
        recursive: true,
    };
    assert!(data.validate(&spec.mounts).is_ok());
    let mut all = spec.mounts.clone();
    all.push(composed);
    assert!(data.validate(&all).is_err());
}
