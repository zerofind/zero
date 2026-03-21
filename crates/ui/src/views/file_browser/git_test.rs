use super::{GitFileStatus, classify_status};

#[test]
fn classify_modified_statuses() {
    assert_eq!(
        classify_status(git2::Status::WT_MODIFIED),
        Some(GitFileStatus::Modified)
    );
    assert_eq!(
        classify_status(git2::Status::INDEX_MODIFIED),
        Some(GitFileStatus::Modified)
    );
    assert_eq!(
        classify_status(git2::Status::INDEX_RENAMED),
        Some(GitFileStatus::Modified)
    );
    assert_eq!(
        classify_status(git2::Status::CONFLICTED),
        Some(GitFileStatus::Modified)
    );
}

#[test]
fn classify_new_statuses() {
    assert_eq!(
        classify_status(git2::Status::WT_NEW),
        Some(GitFileStatus::New)
    );
    assert_eq!(
        classify_status(git2::Status::INDEX_NEW),
        Some(GitFileStatus::New)
    );
}

#[test]
fn classify_deleted_statuses() {
    assert_eq!(
        classify_status(git2::Status::WT_DELETED),
        Some(GitFileStatus::Deleted)
    );
    assert_eq!(
        classify_status(git2::Status::INDEX_DELETED),
        Some(GitFileStatus::Deleted)
    );
}

#[test]
fn classify_ignored_status() {
    assert_eq!(
        classify_status(git2::Status::IGNORED),
        Some(GitFileStatus::Ignored)
    );
}

#[test]
fn classify_current_returns_none() {
    assert_eq!(classify_status(git2::Status::CURRENT), None);
}
