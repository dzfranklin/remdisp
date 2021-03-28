#[cfg(feature = "control")]
pub mod control;

#[cfg(feature = "display")]
pub mod display;

#[cfg(test)]
pub mod tests {
    use std::{fs, io};
    use std::fs::File;
    use std::io::{Write, Read};
    use std::path::Path;

    use evdi::prelude::*;
    use lazy_static::lazy_static;

    use crate::prelude::*;

    use super::*;

    lazy_static! {
        static ref FRAMEBUFS_DIR: &'static Path = &Path::new("sample_data/evdi_framebufs");
    }

    pub(crate) fn mode_fixture() -> Mode {
        let mode_f = File::open(FRAMEBUFS_DIR.join("mode.json"))
            .expect("Do you need to run generate_sample_data?");
        serde_json::from_reader(mode_f).unwrap()
    }

    pub(crate) fn framebuf_fixture(n: u32) -> Vec<u8> {
        let mut buf = vec![];
        File::open(format!("sample_data/evdi_framebufs/{}.framebuf", n))
            .expect("Nonexistent framebuf data")
            .read_to_end(&mut buf).unwrap();
        buf
    }

    #[cfg(feature = "control")]
    #[ignore]
    #[ltest(atest)]
    async fn generate_sample_data() {
        let config = DeviceConfig::sample();
        let mut handle = DeviceNode::get().unwrap().open().unwrap().connect(&config);
        let mode = handle.events.await_mode(TIMEOUT).await.unwrap();
        let buf_id = handle.new_buffer(&mode);

        if let Err(err) = fs::create_dir(*FRAMEBUFS_DIR) {
            if err.kind() != io::ErrorKind::AlreadyExists {
                Err(err).unwrap()
            }
        }

        let mode_data = serde_json::to_vec(&mode).unwrap();
        File::create(FRAMEBUFS_DIR.join("mode.json")).unwrap().write_all(&mode_data).unwrap();

        for _ in 0..200 {
            handle.request_update(buf_id, TIMEOUT).await.unwrap();
        }

        for n in 0..10 {
            handle.request_update(buf_id, TIMEOUT).await.unwrap();
            let mut f = File::create(FRAMEBUFS_DIR.join(format!("{}.framebuf", n))).unwrap();
            f.write_all(handle.get_buffer(buf_id).unwrap().bytes()).unwrap();
        }
    }
}
