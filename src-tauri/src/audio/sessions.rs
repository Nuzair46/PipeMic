use super::{
    AudioResult,
    types::{AudioSession, SessionState},
};

#[cfg(windows)]
use super::windows_wasapi;

pub fn list_sessions() -> AudioResult<Vec<AudioSession>> {
    let sessions = {
        #[cfg(windows)]
        {
            windows_wasapi::list_sessions()?
        }

        #[cfg(not(windows))]
        {
            Vec::new()
        }
    };

    Ok(filter_selectable_sessions(sessions))
}

pub fn filter_selectable_sessions(sessions: Vec<AudioSession>) -> Vec<AudioSession> {
    sessions
        .into_iter()
        .filter(is_selectable_application_session)
        .collect()
}

pub fn is_selectable_application_session(session: &AudioSession) -> bool {
    let executable = session.executable.trim();
    session.state != SessionState::Expired
        && executable.to_ascii_lowercase().ends_with(".exe")
        && !is_self_process(executable)
        && !executable.contains('\\')
        && !executable.contains('/')
}

fn is_self_process(executable: &str) -> bool {
    matches!(executable.to_ascii_lowercase().as_str(), "pipemic.exe")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(executable: &str, state: SessionState, excluded: bool) -> AudioSession {
        AudioSession {
            id: format!("session:{executable}:1"),
            display_name: executable.trim_end_matches(".exe").to_string(),
            executable: executable.to_string(),
            process_id: 1,
            state,
            is_excluded_default: excluded,
        }
    }

    #[test]
    fn filters_to_clear_exe_sessions_and_hides_self() {
        let sessions = filter_selectable_sessions(vec![
            session("Spotify.exe", SessionState::Active, false),
            session("Discord.exe", SessionState::Active, true),
            session("VRChat.exe", SessionState::Active, true),
            session("pipemic.exe", SessionState::Active, false),
            session("System Audio", SessionState::Active, false),
            session("Game.exe", SessionState::Expired, false),
            session("C:\\Windows\\bad.exe", SessionState::Active, false),
        ]);

        assert_eq!(sessions.len(), 3);
        assert_eq!(sessions[0].executable, "Spotify.exe");
        assert_eq!(sessions[1].executable, "Discord.exe");
        assert_eq!(sessions[2].executable, "VRChat.exe");
    }
}
