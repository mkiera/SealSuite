use serde::Deserialize;

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct AppProfile {
    pub display_name: String,
    pub repository: String,
    pub package_id: String,
    pub executable: String,
}

#[allow(dead_code)]
pub fn bundled() -> AppProfile {
    serde_json::from_str(include_str!("../../app_profile.json")).expect("app_profile.json is valid")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_profile_names_sealsuite() {
        let profile = bundled();
        assert_eq!(profile.display_name, "SealSuite");
        assert_eq!(profile.executable, "SealSuite.exe");
    }

    #[test]
    fn package_id_is_a_braced_guid() {
        let id = bundled().package_id;
        assert!(id.starts_with('{') && id.ends_with('}'), "{id}");
        let groups: Vec<&str> = id[1..id.len() - 1].split('-').collect();
        let lengths: Vec<usize> = groups.iter().map(|g| g.len()).collect();
        assert_eq!(lengths, [8, 4, 4, 4, 12]);
        assert!(groups.iter().all(|g| g.chars().all(|c| c.is_ascii_hexdigit())));
    }
}
