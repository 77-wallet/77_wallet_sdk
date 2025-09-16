use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

pub struct SizeRotatingWriter {
    inner: Arc<Mutex<InnerWriter>>,
}

struct InnerWriter {
    base_path: PathBuf,
    current_file: File,
}
impl SizeRotatingWriter {
    pub const MAX_FILES: usize = 3;
    pub const MAX_SIZE: u64 = 1024 * 1024 * 7;

    pub fn new(base_path: PathBuf) -> Result<Self, crate::error::system::SystemError> {
        let file = Self::create_file(base_path.clone())?;
        Ok(Self { inner: Arc::new(Mutex::new(InnerWriter { base_path, current_file: file })) })
    }

    pub fn create_file(base_path: PathBuf) -> io::Result<File> {
        let file_existed = base_path.exists();
        let mut file = OpenOptions::new().create(true).append(true).open(&base_path)?;

        // 如果是新文件或者空文件，写入时间戳
        if !file_existed {
            let timestamp = chrono::Local::now().timestamp();
            writeln!(file, "{}", timestamp)?;
        }
        Ok(file)
    }
}

impl Write for SizeRotatingWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Mutex poisoned: {}", e)))?;

        let metadata = inner.current_file.metadata()?;
        if metadata.len() >= Self::MAX_SIZE {
            rotate_files(&inner.base_path, Self::MAX_FILES)?;
            inner.current_file = Self::create_file(inner.base_path.clone())?;
        }

        inner.current_file.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Mutex poisoned: {}", e)))?;
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
