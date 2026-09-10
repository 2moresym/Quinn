use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{env, fs, io::{self, Read, Write}, path::{Path, PathBuf}, process::Command, thread, time::Duration};

const API: &str = "https://api.github.com/repos/2moresym/Quinn/releases/latest";
const ASSET: &str = "quinn-v0.1.0-beta.1-linux-x86_64.tar.gz";
const CHECKSUM_ASSET: &str = "quinn-v0.1.0-beta.1-linux-x86_64.tar.gz.sha256";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    let install_dir = args.get(1).map(PathBuf::from).unwrap_or_else(|| {
        env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default()
            .join(".local/bin")
    });

    let latest: Value = ureq::get(API)
        .header("User-Agent", "Quinn-Updater")
        .call()?
        .body_mut()
        .read_json()?;
    let tag = latest["tag_name"].as_str().ok_or("release has no tag_name")?;
    let archive_url = asset_url(&latest, ASSET)?;
    let checksum_url = asset_url(&latest, CHECKSUM_ASSET)?;

    println!("latest={tag}");
    let temp = env::temp_dir().join(format!("quinn-update-{}", std::process::id()));
    fs::create_dir_all(&temp)?;
    let archive = temp.join("quinn.tar.gz");
    let checksum = temp.join("quinn.sha256");

    download_with_progress(&archive_url, &archive, "Downloading Quinn")?;
    download_with_progress(&checksum_url, &checksum, "Downloading checksum")?;
    verify_sha256(&archive, &checksum)?;

    println!("progress=extracting");
    let extract = temp.join("extract");
    fs::create_dir_all(&extract)?;
    let status = Command::new("tar")
        .args(["-xzf"])
        .arg(&archive)
        .arg("-C")
        .arg(&extract)
        .status()?;
    if !status.success() { return Err("failed to extract Quinn update".into()); }

    let bundle = extract.join("Quinn");
    fs::create_dir_all(&install_dir)?;
    for name in ["quinn-app", "quinn-daemon", "libvosk.so"] {
        let src = bundle.join(name);
        if src.is_file() {
            let dst = install_dir.join(name);
            let tmp_dst = install_dir.join(format!(".{name}.new"));
            fs::copy(&src, &tmp_dst)?;
            fs::rename(&tmp_dst, &dst)?;
            if name != "libvosk.so" {
                let _ = Command::new("chmod").args(["0755"]).arg(&dst).status();
            }
        }
    }
    println!("progress=complete");
    let _ = fs::remove_dir_all(&temp);
    Ok(())
}

fn asset_url(release: &Value, wanted: &str) -> Result<String, Box<dyn std::error::Error>> {
    release["assets"].as_array()
        .and_then(|assets| assets.iter().find(|a| a["name"].as_str() == Some(wanted)))
        .and_then(|a| a["browser_download_url"].as_str())
        .map(str::to_owned)
        .ok_or_else(|| format!("release asset not found: {wanted}").into())
}

fn download_with_progress(url: &str, path: &Path, label: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut response = ureq::get(url).header("User-Agent", "Quinn-Updater").call()?;
    let total = response.headers().get("content-length").and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<u64>().ok());
    let mut reader = response.body_mut().as_reader();
    let mut file = fs::File::create(path)?;
    let mut buf = [0u8; 64 * 1024];
    let mut done = 0u64;
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 { break; }
        file.write_all(&buf[..n])?;
        done += n as u64;
        if let Some(total) = total {
            println!("progress={}:{}", label.replace(' ', "_"), done.saturating_mul(100) / total.max(1));
        } else {
            println!("progress={}:{}", label.replace(' ', "_"), done);
        }
    }
    Ok(())
}

fn verify_sha256(path: &Path, checksum_file: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let expected_line = fs::read_to_string(checksum_file)?;
    let expected = expected_line.split_whitespace().next().ok_or("checksum file is empty")?;
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 { break; }
        hasher.update(&buf[..n]);
    }
    let actual = format!("{:x}", hasher.finalize());
    if actual != expected { return Err("Quinn update checksum verification failed".into()); }
    println!("checksum=ok");
    Ok(())
}
