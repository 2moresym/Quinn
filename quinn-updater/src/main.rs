use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    env,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
};

const API: &str = "https://api.github.com/repos/2moresym/Quinn/releases/latest";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let install_dir = env::args().nth(1).map(PathBuf::from).unwrap_or_else(|| {
        env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default()
            .join(".local/bin")
    });

    let mut response = ureq::get(API)
        .header("User-Agent", "Quinn-Updater")
        .call()?;
    let mut json = Vec::new();
    response.body_mut().as_reader().read_to_end(&mut json)?;
    let release: Value = serde_json::from_slice(&json)?;

    let tag = release["tag_name"]
        .as_str()
        .ok_or("latest release has no tag_name")?;
    let asset_name = format!("quinn-{tag}-linux-x86_64.tar.gz");
    let checksum_name = format!("{asset_name}.sha256");
    let archive_url = asset_url(&release, &asset_name)?;
    let checksum_url = asset_url(&release, &checksum_name)?;

    println!("latest={tag}");
    let temp = env::temp_dir().join(format!("quinn-update-{}", std::process::id()));
    fs::create_dir_all(&temp)?;
    let archive = temp.join(&asset_name);
    let checksum = temp.join(&checksum_name);
    download(&archive_url, &archive)?;
    download(&checksum_url, &checksum)?;
    verify_sha256(&archive, &checksum)?;

    let extract = temp.join("extract");
    fs::create_dir_all(&extract)?;
    let status = Command::new("tar")
        .args(["-xzf"])
        .arg(&archive)
        .arg("-C")
        .arg(&extract)
        .status()?;
    if !status.success() {
        return Err("failed to extract Quinn update".into());
    }

    let bundle = extract.join("Quinn");
    fs::create_dir_all(&install_dir)?;
    for name in ["quinn-app", "quinn-daemon", "quinn-updater"] {
        let src = bundle.join(name);
        if !src.is_file() {
            return Err(format!("release is missing required binary: {name}").into());
        }
        atomic_replace(&src, &install_dir.join(name), 0o755)?;
    }

    if bundle.join("libvosk.so").is_file() {
        let data_dir = install_dir
            .parent()
            .unwrap_or(&install_dir)
            .join("share/quinn/vosk");
        fs::create_dir_all(&data_dir)?;
        atomic_replace(
            &bundle.join("libvosk.so"),
            &data_dir.join("libvosk.so"),
            0o644,
        )?;
    }

    if bundle.join("quinn.svg").is_file() {
        let icon_dir = install_dir
            .parent()
            .unwrap_or(&install_dir)
            .join("share/icons/hicolor/scalable/apps");
        fs::create_dir_all(&icon_dir)?;
        atomic_replace(&bundle.join("quinn.svg"), &icon_dir.join("quinn.svg"), 0o644)?;
    }

    fs::remove_dir_all(temp).ok();
    println!("updated={tag}");
    Ok(())
}

fn asset_url(release: &Value, name: &str) -> Result<String, Box<dyn std::error::Error>> {
    release["assets"]
        .as_array()
        .and_then(|assets| assets.iter().find(|a| a["name"].as_str() == Some(name)))
        .and_then(|asset| asset["browser_download_url"].as_str())
        .map(str::to_owned)
        .ok_or_else(|| format!("release asset not found: {name}").into())
}

fn download(url: &str, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut response = ureq::get(url).header("User-Agent", "Quinn-Updater").call()?;
    let mut reader = response.body_mut().as_reader();
    let mut file = fs::File::create(path)?;
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])?;
    }
    Ok(())
}

fn verify_sha256(path: &Path, checksum: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let expected = fs::read_to_string(checksum)?
        .split_whitespace()
        .next()
        .ok_or("empty checksum")?
        .to_owned();
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let actual = format!("{:x}", hasher.finalize());
    if actual != expected {
        return Err("Quinn update checksum verification failed".into());
    }
    Ok(())
}

fn atomic_replace(source: &Path, destination: &Path, mode: u32) -> Result<(), Box<dyn std::error::Error>> {
    let temp = destination.with_extension(format!("new.{}", std::process::id()));
    fs::copy(source, &temp)?;
    fs::set_permissions(&temp, fs::Permissions::from_mode(mode))?;
    fs::rename(temp, destination)?;
    Ok(())
}

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
