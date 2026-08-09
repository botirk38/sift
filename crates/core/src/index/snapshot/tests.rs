use super::store::disk::DiskSnapshotStore;
use super::*;
use tempfile::TempDir;

#[test]
fn disk_begin_commit_creates_current() {
    let tmp = TempDir::new().expect("create temp dir");
    let mut store = DiskSnapshotStore::open(tmp.path()).expect("open store");
    assert!(store.current_id().is_none());

    {
        let mut session = store.writer().expect("writer");
        let mut txn = session.begin().expect("begin");
        txn.put_artifact("test", "data.txt", b"hello".to_vec())
            .expect("put artifact");

        let manifest = SnapshotManifest {
            id: txn.id().clone(),
            indexes: vec![],
        };
        session.publish(txn, &manifest).expect("publish");
    }

    assert!(store.current_id().is_some());
    let current_id = store.current_id().unwrap().to_string();
    let snap_dir = tmp.path().join("snapshots").join(&current_id);
    assert!(snap_dir.exists());
    assert!(snap_dir.join("test").join("data.txt").exists());

    let store2 = DiskSnapshotStore::open(tmp.path()).expect("reopen");
    assert_eq!(
        store2.current_id().map(SnapshotId::as_str),
        store.current_id().map(SnapshotId::as_str),
    );
}

#[test]
fn disk_drop_without_commit_cleans_tmp() {
    let tmp = TempDir::new().expect("create temp dir");
    let mut store = DiskSnapshotStore::open(tmp.path()).expect("open store");

    let mut session = store.writer().expect("writer");
    let txn = session.begin().expect("begin");
    let tmp_dir = txn.dir.clone();
    assert!(tmp_dir.exists());
    drop(txn);
    assert!(!tmp_dir.exists());
}

#[test]
fn disk_writer_sees_new_current() {
    let tmp = TempDir::new().expect("create temp dir");
    let mut store = DiskSnapshotStore::open(tmp.path()).expect("open store");

    {
        let mut session = store.writer().expect("writer");
        let txn = session.begin().expect("begin");
        let manifest = SnapshotManifest {
            id: txn.id().clone(),
            indexes: vec![],
        };
        let id1 = session.publish(txn, &manifest).expect("publish");
        let cur = session.current().unwrap().expect("current after publish");
        assert_eq!(cur.manifest().id, id1);
    }

    {
        let mut session = store.writer().expect("writer");
        let prev = session.current().unwrap().expect("prev");
        let txn = session.begin().expect("begin");
        let manifest = SnapshotManifest {
            id: txn.id().clone(),
            indexes: vec![],
        };
        let id2 = session.publish(txn, &manifest).expect("publish");
        let cur = session.current().unwrap().expect("current after publish");
        assert_eq!(cur.manifest().id, id2);
        assert_ne!(id2, prev.manifest().id);
    }
}
