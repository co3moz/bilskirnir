//! Pins down what the manager reads from a host's `/sys`: the WSL GPU device, an NVIDIA display
//! device only while NVIDIA's own driver runs it, render devices only with a PCI vendor and
//! another driver, and the NVIDIA Container Toolkit as `docker info` names it.
use super::*;
use serde_json::json;

/// A `/sys` of its own, removed when the test ends.
struct Sys(PathBuf);

impl Sys {
    fn new() -> Sys {
        let root = std::env::temp_dir().join(format!("thoric-sys-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("class")).unwrap();
        Sys(root)
    }

    fn write(&self, path: &str, text: &str) -> &Sys {
        let path = self.0.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
        self
    }

    /// A PCI device, as `bus/pci/devices` lists it.
    fn pci(&self, slot: &str, vendor: &str, class: &str, driver: &str) -> &Sys {
        let device = format!("bus/pci/devices/{slot}");
        self.write(&format!("{device}/vendor"), &format!("{vendor}\n"))
            .write(&format!("{device}/class"), &format!("{class}\n"))
            .write(
                &format!("{device}/uevent"),
                &format!("DRIVER={driver}\nPCI_CLASS={}\n", &class[2..]),
            )
    }

    /// A render node whose device has a PCI vendor, or none for a virtual one.
    fn render(&self, node: &str, vendor: Option<&str>, driver: &str) -> &Sys {
        let device = format!("class/drm/{node}/device");
        self.write(&format!("{device}/uevent"), &format!("DRIVER={driver}\n"));
        if let Some(vendor) = vendor {
            self.write(&format!("{device}/vendor"), &format!("{vendor}\n"));
        }
        self
    }

    fn read(&self) -> Seen {
        Seen::read(&self.0).unwrap()
    }
}

impl Drop for Sys {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn wsl_shows_its_gpu_device_and_no_render_device() {
    // As a WSL distribution with an NVIDIA driver on Windows shows it: the paravirtual adapter is
    // Microsoft's, and /dev/dri is vgem.
    let sys = Sys::new();
    sys.write("class/misc/dxg/dev", "10:258\n")
        .pci("06d1:00:00.0", "0x1414", "0x030200", "dxgkrnl")
        .pci("2cf5:00:00.0", "0x1af4", "0x088000", "virtio-pci")
        .render("renderD128", None, "vgem");
    assert_eq!(
        sys.read(),
        Seen {
            wsl: true,
            ..Default::default()
        }
    );
}

#[test]
fn an_nvidia_gpu_counts_only_with_nvidias_driver() {
    let sys = Sys::new();
    sys.pci("0000:01:00.0", "0x10de", "0x030000", "nvidia")
        .pci("0000:01:00.1", "0x10de", "0x040300", "snd_hda_intel")
        // NVIDIA's DRM node renders only through the toolkit.
        .render("renderD128", Some("0x10de"), "nvidia");
    assert_eq!(
        sys.read(),
        Seen {
            nvidia: true,
            ..Default::default()
        }
    );
    // A datacenter GPU is a 3D controller.
    let sys = Sys::new();
    sys.pci("0000:3b:00.0", "0x10de", "0x030200", "nvidia");
    assert!(sys.read().nvidia);
    // With nouveau there is nothing for the toolkit, but Mesa renders on it.
    let sys = Sys::new();
    sys.pci("0000:01:00.0", "0x10de", "0x030000", "nouveau")
        .render("renderD128", Some("0x10de"), "nouveau");
    assert_eq!(
        sys.read(),
        Seen {
            render: vec!["NVIDIA".into()],
            ..Default::default()
        }
    );
}

#[test]
fn render_devices_are_named_by_vendor_once_each() {
    let sys = Sys::new();
    sys.pci("0000:00:02.0", "0x8086", "0x030000", "i915")
        .pci("0000:03:00.0", "0x1002", "0x030000", "amdgpu")
        .render("renderD128", Some("0x8086"), "i915")
        .render("renderD129", Some("0x1002"), "amdgpu")
        .render("renderD130", Some("0x1002"), "amdgpu")
        .render("renderD131", Some("0x1af4"), "virtio-pci")
        // A primary node is not a render node.
        .render("card0", Some("0x8086"), "i915");
    assert_eq!(sys.read().render, ["0x1af4", "AMD", "Intel"]);
}

#[test]
fn a_hybrid_laptop_shows_both() {
    let sys = Sys::new();
    sys.pci("0000:00:02.0", "0x8086", "0x030000", "i915")
        .pci("0000:01:00.0", "0x10de", "0x030200", "nvidia")
        .render("renderD128", Some("0x8086"), "i915")
        .render("renderD129", Some("0x10de"), "nvidia");
    let seen = sys.read();
    assert!(seen.nvidia && !seen.wsl);
    assert_eq!(seen.render, ["Intel"]);
}

#[test]
fn nothing_is_seen_without_a_sys() {
    let missing = std::env::temp_dir().join(format!("thoric-sys-{}", uuid::Uuid::new_v4()));
    assert_eq!(Seen::read(&missing), None);
    // A host without GPUs is still seen.
    assert_eq!(Sys::new().read(), Seen::default());
}

#[test]
fn the_toolkit_is_its_runtime_or_a_cdi_device() {
    assert!(toolkit(
        &json!({"Runtimes": {"io.containerd.runc.v2": {}, "nvidia": {"path": "nvidia-container-runtime"}}})
    ));
    assert!(toolkit(
        &json!({"Runtimes": {"runc": {}}, "DiscoveredDevices": [{"Source": "cdi", "ID": "nvidia.com/gpu=all"}]})
    ));
    assert!(!toolkit(
        &json!({"Runtimes": {"runc": {}}, "DiscoveredDevices": [{"Source": "cdi", "ID": "amd.com/gpu=all"}]})
    ));
    // Docker before 28.3 lists no CDI devices.
    assert!(!toolkit(&json!({"Runtimes": {"runc": {}}})));
}

#[tokio::test]
async fn a_remote_daemon_shows_nothing() {
    let mut app = crate::tests::app(false);
    let config = &mut std::sync::Arc::get_mut(&mut app).unwrap().config;
    config.docker_socket = None;
    assert_eq!(Host::detect(config).await, Host::default());
}
