use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use chrono::Local;

pub struct SizeRotatingWriter {
    inner: Arc<Mutex<InnerWriter>>,
}

struct InnerWriter {
    base_path: PathBuf,
    max_size: u64,
    max_files: usize,
    current_file: File,
}
impl SizeRotatingWriter {
    pub fn new(
        base_path: PathBuf,
        max_size: u64,
        max_files: usize,
    ) -> Result<Self, crate::SystemError> {
        let file = Self::create_file(base_path.clone())?;
        Ok(Self {
            inner: Arc::new(Mutex::new(InnerWriter {
                base_path,
                max_size,
                max_files,
                current_file: file,
            })),
        })
    }

    pub fn create_file(base_path: PathBuf) -> io::Result<File> {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&base_path)?;
        let timestamp = Local::now().timestamp();
        writeln!(file, "{}", timestamp)?;
        Ok(file)
    }
}

impl Write for SizeRotatingWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut inner = self.inner.lock().unwrap();

        let metadata = inner.current_file.metadata()?;
        if metadata.len() >= inner.max_size {
            rotate_files(&inner.base_path, inner.max_files)?;
            inner.current_file = Self::create_file(inner.base_path.clone())?;
        }

        inner.current_file.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        let mut inner = self.inner.lock().unwrap();
        inner.current_file.flush()
    }
}
fn rotate_files(base_path: &Path, max_files: usize) -> io::Result<()> {
    let oldest = base_path.with_extension(format!("{}.txt", max_files));

    if oldest.exists() {
        fs::remove_file(&oldest)?;
    }

    for i in (1..=max_files - 1).rev() {
        let from = base_path.with_extension(format!("{}.txt", i));
        let to = base_path.with_extension(format!("{}.txt", i + 1));
        if from.exists() {
            fs::rename(from, to)?;
        }
    }

    let rotated = base_path.with_extension("1.txt");
    if base_path.exists() {
        fs::rename(base_path, rotated)?;
    }

    Ok(())
}
