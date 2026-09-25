use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreAudioError {
    #[error("{context} failed: OSStatus {status} ({fourcc})")]
    OsStatus {
        context: &'static str,
        status: i32,
        fourcc: String,
    },
    #[error("invalid path: {0}")]
    InvalidPath(std::path::PathBuf),
    #[error("audio component not found: {0}")]
    ComponentNotFound(&'static str),
    #[error("unsupported format: {0}")]
    Unsupported(String),
}

pub type Result<T> = std::result::Result<T, CoreAudioError>;

/// Checks the OSStatus returned by Core Audio APIs.
pub fn check(status: i32, context: &'static str) -> Result<()> {
    if status == 0 {
        Ok(())
    } else {
        let bytes = status.to_be_bytes();
        let is_ascii = bytes.iter().all(|&b| (0x20..=0x7E).contains(&b));
        let fourcc = if is_ascii {
            let s: String = bytes.iter().map(|&b| b as char).collect();
            format!("'{}'", s)
        } else {
            "".to_string()
        };
        Err(CoreAudioError::OsStatus {
            context,
            status,
            fourcc,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_ok() {
        assert!(check(0, "test").is_ok());
    }

    #[test]
    fn test_check_err() {
        assert!(check(-50, "test").is_err());
    }

    #[test]
    fn test_check_fourcc() {
        // 0x7768743f is 'wht?' in big-endian
        // 0x77='w', 0x68='h', 0x74='t', 0x3f='?'
        let result = check(0x7768743f, "test");
        match result {
            Err(CoreAudioError::OsStatus { fourcc, .. }) => assert_eq!(fourcc, "'wht?'"),
            _ => panic!("Expected OsStatus error"),
        }
    }
}
