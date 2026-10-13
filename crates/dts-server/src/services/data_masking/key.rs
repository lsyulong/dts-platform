use base64::{Engine, engine::general_purpose::STANDARD};
use ring::{
    hmac,
    rand::{SecureRandom, SystemRandom},
};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Clone)]
pub struct PrivacyKey(Arc<hmac::Key>);

impl PrivacyKey {
    pub fn load_or_create(data_dir: &str) -> anyhow::Result<Self> {
        Self::load_named(data_dir, "privacy-hmac.key")
    }
    pub(crate) fn load_named(data_dir: &str, filename: &str) -> anyhow::Result<Self> {
        if !matches!(filename, "privacy-hmac.key" | "service-credential-hmac.key") {
            anyhow::bail!("服务器密钥文件名无效");
        }
        let path = PathBuf::from(data_dir).join(filename);
        reject_symlink(&path)?;
        match File::open(&path) {
            Ok(file) => {
                restrict(&path)?;
                return Self::read(file);
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {},
            Err(error) => return Err(error.into()),
        }
        std::fs::create_dir_all(
            path.parent()
                .ok_or_else(|| anyhow::anyhow!("脱敏密钥目录无效"))?,
        )?;
        let mut key = [0u8; 32];
        SystemRandom::new()
            .fill(&mut key)
            .map_err(|_| anyhow::anyhow!("脱敏密钥随机数生成失败"))?;
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&path) {
            Ok(mut file) => {
                restrict(&path)?;
                file.write_all(STANDARD.encode(key).as_bytes())?;
                file.sync_all()?;
                Ok(Self::from_bytes(key))
            },
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                reject_symlink(&path)?;
                restrict(&path)?;
                Self::read(File::open(&path)?)
            },
            Err(error) => Err(error.into()),
        }
    }

    pub(super) fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(Arc::new(hmac::Key::new(hmac::HMAC_SHA256, &bytes)))
    }
    #[cfg(test)]
    pub(crate) fn from_test_bytes(bytes: [u8; 32]) -> Self {
        Self::from_bytes(bytes)
    }
    fn read(file: File) -> anyhow::Result<Self> {
        if !file.metadata()?.is_file() {
            anyhow::bail!("脱敏密钥必须是普通文件");
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if file.metadata()?.permissions().mode() & 0o077 != 0 {
                anyhow::bail!("脱敏密钥文件权限须为0600");
            }
        }
        let mut encoded = String::new();
        file.take(129).read_to_string(&mut encoded)?;
        if encoded.len() > 128 {
            anyhow::bail!("脱敏密钥文件格式无效");
        }
        let bytes: [u8; 32] = STANDARD
            .decode(encoded.trim())?
            .try_into()
            .map_err(|_| anyhow::anyhow!("脱敏密钥必须为32字节"))?;
        Ok(Self::from_bytes(bytes))
    }
    pub(super) fn sign(&self, bytes: &[u8]) -> String {
        STANDARD.encode(hmac::sign(&self.0, bytes).as_ref())
    }
    pub(super) fn verify(&self, bytes: &[u8], signature: &str) -> bool {
        STANDARD
            .decode(signature)
            .is_ok_and(|tag| hmac::verify(&self.0, bytes, &tag).is_ok())
    }
    pub(crate) fn sign_hex(&self, bytes: &[u8]) -> String {
        hmac::sign(&self.0, bytes)
            .as_ref()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }
    pub(crate) fn verify_hex(&self, bytes: &[u8], signature: &str) -> bool {
        if signature.len() != 64 {
            return false;
        }
        let tag: Option<Vec<u8>> = signature
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| {
                let text = std::str::from_utf8(pair).ok()?;
                u8::from_str_radix(text, 16).ok()
            })
            .collect();
        tag.is_some_and(|tag| hmac::verify(&self.0, bytes, &tag).is_ok())
    }
}

fn reject_symlink(path: &Path) -> anyhow::Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            anyhow::bail!("脱敏密钥不能是链接或目录")
        },
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

#[cfg(not(windows))]
fn restrict(_path: &Path) -> anyhow::Result<()> {
    Ok(())
}

#[cfg(windows)]
fn restrict(path: &Path) -> anyhow::Result<()> {
    use std::os::windows::process::CommandExt;
    // The path is passed as a child environment value, never interpolated into a shell command.
    // A fresh descriptor changes only the owner and DACL; reapplying Get-Acl's
    // descriptor can request SeSecurityPrivilege for unrelated audit metadata.
    let script = "$ErrorActionPreference='Stop'; $p=$env:DTS_PRIVACY_KEY_PATH; $acl=[System.Security.AccessControl.FileSecurity]::new(); $acl.SetAccessRuleProtection($true,$false); $sid=[System.Security.Principal.WindowsIdentity]::GetCurrent().User; $acl.SetOwner($sid); $acl.AddAccessRule([System.Security.AccessControl.FileSystemAccessRule]::new($sid,'FullControl','Allow')); $system=[System.Security.Principal.SecurityIdentifier]::new('S-1-5-18'); $acl.AddAccessRule([System.Security.AccessControl.FileSystemAccessRule]::new($system,'FullControl','Allow')); [System.IO.File]::SetAccessControl($p,$acl)";
    let output = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        // Windows PowerShell must discover its own modules, not inherited PowerShell 7 modules.
        .env_remove("PSModulePath")
        .env("DTS_PRIVACY_KEY_PATH", path)
        .creation_flags(0x08000000)
        .output()?;
    if !output.status.success() {
        anyhow::bail!("无法限制脱敏密钥文件ACL，请检查运行账户权限");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_key_creation_and_restart_preserve_signatures() {
        let directory = tempfile::tempdir().expect("isolated key directory");
        let first = PrivacyKey::load_or_create(directory.path().to_str().expect("UTF-8 path"))
            .expect("create and restrict actual key");
        let signature = first.sign(b"non-secret regression message");
        let loaded = PrivacyKey::load_or_create(directory.path().to_str().expect("UTF-8 path"))
            .expect("restrict and reload actual key");
        assert!(loaded.verify(b"non-secret regression message", &signature));
        assert!(!loaded.verify(b"different message", &signature));
    }
}
