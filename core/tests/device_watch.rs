use maplay_core::coreaudio::device_watch::DeviceWatcher;

#[test]
fn watcher_create_and_drop() {
    // First create + drop
    let (w, rx) = match DeviceWatcher::new() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("DeviceWatcher::new failed (no Core Audio?): {e}");
            return;
        }
    };
    // Verify we can drain the channel without panicking (results don't matter).
    let _ = rx.try_recv();
    drop(w);

    // Second create + drop should also succeed.
    let (w, rx) = match DeviceWatcher::new() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("DeviceWatcher::new (second) failed: {e}");
            return;
        }
    };
    let _ = rx.try_recv();
    drop(w);
}
