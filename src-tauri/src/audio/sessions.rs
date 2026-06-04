use super::{
    AudioResult,
    types::{AppDiscoverySource, AudioSession, SessionState},
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
    let mut sessions: Vec<_> = sessions
        .into_iter()
        .filter(is_selectable_application_session)
        .collect();

    sessions.sort_by(|left, right| {
        session_rank(right)
            .cmp(&session_rank(left))
            .then_with(|| session_label(left).cmp(&session_label(right)))
    });

    merge_sessions_by_executable(sessions)
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

fn merge_sessions_by_executable(sessions: Vec<AudioSession>) -> Vec<AudioSession> {
    let mut merged: Vec<AudioSession> = Vec::new();
    for session in sessions {
        if let Some(existing) = merged
            .iter_mut()
            .find(|existing| same_executable(&existing.executable, &session.executable))
        {
            merge_session(existing, session);
        } else {
            merged.push(session);
        }
    }

    merged.sort_by(|left, right| {
        session_rank(right)
            .cmp(&session_rank(left))
            .then_with(|| session_label(left).cmp(&session_label(right)))
    });
    merged
}

fn merge_session(existing: &mut AudioSession, next: AudioSession) {
    if existing.state != SessionState::Active && next.state == SessionState::Active {
        existing.id = next.id.clone();
        existing.display_name = next.display_name.clone();
        existing.process_id = next.process_id;
        existing.state = next.state;
    }

    existing.has_audio_session = existing.has_audio_session || next.has_audio_session;
    existing.window_title = best_window_title(existing, &next);
    existing.discovery_source = match (existing.has_audio_session, existing.window_title.is_some()) {
        (true, true) => AppDiscoverySource::Merged,
        (true, false) => AppDiscoverySource::AudioSession,
        (false, _) => AppDiscoverySource::Window,
    };

    if existing.window_title.is_none()
        && existing.display_name.trim().is_empty()
        && !next.display_name.trim().is_empty()
    {
        existing.display_name = next.display_name;
    }
}

fn best_window_title(existing: &AudioSession, next: &AudioSession) -> Option<String> {
    match (&existing.window_title, &next.window_title) {
        (None, None) => None,
        (Some(title), None) | (None, Some(title)) => Some(title.clone()),
        (Some(left), Some(right)) => {
            if window_title_score(right, &next.executable)
                > window_title_score(left, &existing.executable)
            {
                Some(right.clone())
            } else {
                Some(left.clone())
            }
        }
    }
}

fn session_rank(session: &AudioSession) -> u8 {
    let active = if session.state == SessionState::Active { 4 } else { 0 };
    let audio = if session.has_audio_session { 2 } else { 0 };
    let window = if session.window_title.is_some() { 1 } else { 0 };
    active + audio + window
}

fn session_label(session: &AudioSession) -> String {
    session
        .window_title
        .as_deref()
        .filter(|title| !title.trim().is_empty())
        .unwrap_or(&session.display_name)
        .to_ascii_lowercase()
}

fn window_title_score(title: &str, executable: &str) -> usize {
    let executable_stem = executable_stem(executable).to_ascii_lowercase();
    let normalized = title.to_ascii_lowercase();
    let has_context = !normalized.eq(&executable_stem) && normalized.len() > executable_stem.len();
    title.chars().count() + if has_context { 1_000 } else { 0 }
}

fn executable_stem(executable: &str) -> &str {
    executable
        .strip_suffix(".exe")
        .or_else(|| executable.strip_suffix(".EXE"))
        .unwrap_or(executable)
}

fn same_executable(left: &str, right: &str) -> bool {
    left.eq_ignore_ascii_case(right)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn audio_session(executable: &str, state: SessionState) -> AudioSession {
        AudioSession {
            id: format!("session:{executable}:1"),
            display_name: executable_stem(executable).to_string(),
            executable: executable.to_string(),
            process_id: 1,
            state,
            is_excluded_default: false,
            window_title: None,
            has_audio_session: true,
            discovery_source: AppDiscoverySource::AudioSession,
        }
    }

    fn window_session(executable: &str, title: &str) -> AudioSession {
        AudioSession {
            id: format!("window:{executable}:2"),
            display_name: executable_stem(executable).to_string(),
            executable: executable.to_string(),
            process_id: 2,
            state: SessionState::Inactive,
            is_excluded_default: false,
            window_title: Some(title.to_string()),
            has_audio_session: false,
            discovery_source: AppDiscoverySource::Window,
        }
    }

    #[test]
    fn filters_to_clear_exe_sessions_and_hides_self() {
        let sessions = filter_selectable_sessions(vec![
            audio_session("Spotify.exe", SessionState::Active),
            audio_session("Discord.exe", SessionState::Active),
            audio_session("VRChat.exe", SessionState::Active),
            audio_session("pipemic.exe", SessionState::Active),
            audio_session("System Audio", SessionState::Active),
            audio_session("Game.exe", SessionState::Expired),
            audio_session("C:\\Windows\\bad.exe", SessionState::Active),
        ]);

        assert_eq!(sessions.len(), 3);
        assert!(sessions.iter().any(|session| session.executable == "Spotify.exe"));
        assert!(sessions.iter().any(|session| session.executable == "Discord.exe"));
        assert!(sessions.iter().any(|session| session.executable == "VRChat.exe"));
    }

    #[test]
    fn keeps_window_only_apps_before_audio_starts() {
        let sessions = filter_selectable_sessions(vec![window_session(
            "firefox.exe",
            "YouTube - Mozilla Firefox",
        )]);

        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].executable, "firefox.exe");
        assert_eq!(sessions[0].state, SessionState::Inactive);
        assert_eq!(sessions[0].window_title.as_deref(), Some("YouTube - Mozilla Firefox"));
        assert!(!sessions[0].has_audio_session);
        assert_eq!(sessions[0].discovery_source, AppDiscoverySource::Window);
    }

    #[test]
    fn merges_audio_session_with_visible_window_context() {
        let sessions = filter_selectable_sessions(vec![
            audio_session("msedge.exe", SessionState::Active),
            window_session("msedge.exe", "Twitch - Microsoft Edge"),
        ]);

        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].executable, "msedge.exe");
        assert_eq!(sessions[0].state, SessionState::Active);
        assert_eq!(sessions[0].process_id, 1);
        assert_eq!(sessions[0].window_title.as_deref(), Some("Twitch - Microsoft Edge"));
        assert!(sessions[0].has_audio_session);
        assert_eq!(sessions[0].discovery_source, AppDiscoverySource::Merged);
    }
}
