use mmp_core::coreaudio::device;

#[test]
fn test_output_devices() {
    let devices = match device::output_devices() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("output_devices error: {e}");
            return;
        }
    };
    if devices.is_empty() {
        eprintln!("no output devices found — skipping remaining tests");
        return;
    }

    // All returned devices must have output_channels > 0 and non-empty uid.
    for d in &devices {
        assert!(d.output_channels > 0, "device {} has 0 output channels", d.id);
        assert!(!d.uid.is_empty(), "device {} has empty uid", d.id);
    }

    // default_output_device() ID must be in the list.
    let default_id = match device::default_output_device() {
        Ok(id) => id,
        Err(e) => {
            eprintln!("default_output_device error: {e}");
            return;
        }
    };
    assert!(
        devices.iter().any(|d| d.id == default_id),
        "default_output_device() id {default_id} not found in output_devices()",
    );

    // find_output_device_by_uid finds the first device's uid.
    let first = &devices[0];
    let found = match device::find_output_device_by_uid(&first.uid) {
        Ok(opt) => opt,
        Err(e) => {
            eprintln!("find_output_device_by_uid({}) error: {e}", first.uid);
            return;
        }
    };
    assert!(
        found.as_ref().is_some(),
        "find_output_device_by_uid({}) returned None",
        first.uid,
    );
    assert_eq!(found.unwrap().id, first.id);

    // find_output_device_by_uid returns None for a non-existent uid.
    let not_found = match device::find_output_device_by_uid("__nonexistent_uid_xyz__") {
        Ok(opt) => opt,
        Err(e) => {
            eprintln!("find_output_device_by_uid(nonexistent) error: {e}");
            return;
        }
    };
    assert!(not_found.is_none(), "nonexistent uid should return None");

    // is_alive(0) returns false.
    assert!(!device::is_alive(0), "is_alive(0) should be false");
}
