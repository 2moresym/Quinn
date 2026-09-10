use std::{path::PathBuf, process::Command};

#[derive(Clone, Copy, Debug)]
pub enum VoiceClip {
    Done,
    Sorry,
    Understood,
    TimerSet,
    TimerRemoved,
}

impl VoiceClip {
    fn filename(self) -> &'static str {
        match self {
            Self::Done => "Done.wav",
            Self::Sorry => "Sorry.wav",
            Self::Understood => "Understood.wav",
            Self::TimerSet => "Timer_Set.wav",
            Self::TimerRemoved => "Timer_Removed.wav",
        }
    }
}

#[derive(Clone)]
pub struct VoicePlayer {
    directory: PathBuf,
}

impl VoicePlayer {
    pub fn new() -> Self {
        let directory = std::env::var_os("QUINN_VOICE_DIR")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(PathBuf::from)
                    .map(|home| home.join(".local/share/quinn/voices/female"))
            })
            .unwrap_or_else(|| PathBuf::from("Voices/female"));
        Self { directory }
    }

    pub fn play(&self, clip: VoiceClip) -> Result<(), String> {
        let path = self.directory.join(clip.filename());
        if !path.is_file() {
            return Err(format!("voice clip not found: {}", path.display()));
        }

        for program in ["pw-play", "paplay", "aplay"] {
            if !command_available(program) {
                continue;
            }
            Command::new(program)
                .arg(&path)
                .spawn()
                .map_err(|e| format!("could not start {program}: {e}"))?;
            return Ok(());
        }

        Err("no supported audio player found (pw-play, paplay, or aplay)".to_string())
    }
}

fn command_available(program: &str) -> bool {
    Command::new("sh")
        .args(["-c", "command -v \"$1\" >/dev/null 2>&1", "quinn", program])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}
