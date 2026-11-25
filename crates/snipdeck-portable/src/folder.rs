//! Where a backup folder lives, judged from its path alone (DATA-22): a network share can drop
//! away mid-write, and a sync service uploads every backup (sensitive items inside, encrypted
//! but still a copy) and can resurrect an old one over a new one. Nothing is read from disk, so
//! the answer is the same on every platform and in every test.

use std::path::{Component, Path, Prefix};

/// A reason to think twice about keeping backups here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FolderRisk {
    /// A network location (`\\server\share`, a mapped path on a UNC prefix): it can be offline
    /// when a backup is due, and a half-written file there is hard to tell from a whole one.
    Network,
    /// A folder a sync service watches; the name is the service as people know it.
    Synced(&'static str),
}

/// The services recognised by a folder name on the path, compared without case.
const SYNC_FOLDERS: [(&str, &str); 8] = [
    ("dropbox", "Dropbox"),
    ("google drive", "Google Drive"),
    ("googledrive", "Google Drive"),
    ("icloud drive", "iCloud Drive"),
    ("iclouddrive", "iCloud Drive"),
    ("box sync", "Box"),
    ("box", "Box"),
    ("pcloud drive", "pCloud"),
];

/// What is risky about keeping backups in `folder`, from its path alone; empty when nothing is.
#[must_use]
pub fn folder_risks(folder: &Path) -> Vec<FolderRisk> {
    let mut risks = Vec::new();
    let mut components = folder.components();
    let network = match components.next() {
        Some(Component::Prefix(p)) => {
            matches!(p.kind(), Prefix::UNC(..) | Prefix::VerbatimUNC(..))
        }
        _ => false,
    } || {
        // On other systems (and for a path typed as text) a leading `\\` or `//`.
        let text = folder.to_string_lossy();
        (text.starts_with("\\\\") || text.starts_with("//")) && !text.starts_with("\\\\?\\")
    };
    if network {
        risks.push(FolderRisk::Network);
    }
    for component in folder.to_string_lossy().split(['\\', '/']) {
        let name = component.trim().to_lowercase();
        let service = if name == "onedrive" || name.starts_with("onedrive -") {
            Some("OneDrive")
        } else {
            SYNC_FOLDERS
                .iter()
                .find(|(folder_name, _)| *folder_name == name)
                .map(|(_, service)| *service)
        };
        if let Some(service) = service
            && !risks.contains(&FolderRisk::Synced(service))
        {
            risks.push(FolderRisk::Synced(service));
        }
    }
    risks
}

#[cfg(test)]
mod tests {
    use super::*;

    fn risks(path: &str) -> Vec<FolderRisk> {
        folder_risks(Path::new(path))
    }

    #[test]
    fn a_plain_local_folder_is_fine() {
        assert_eq!(
            risks("C:\\Users\\me\\AppData\\Local\\Snipdeck\\backups"),
            []
        );
        assert_eq!(risks("/home/me/backups"), []);
        assert_eq!(risks("D:\\Backups\\Snipdeck"), []);
    }

    #[test]
    fn a_network_share_is_named() {
        assert_eq!(risks("\\\\nas\\share\\snipdeck"), [FolderRisk::Network]);
        assert_eq!(risks("//nas/share/snipdeck"), [FolderRisk::Network]);
    }

    #[test]
    fn a_long_local_path_is_not_a_network_share() {
        assert_eq!(risks("\\\\?\\C:\\Users\\me\\backups"), []);
    }

    #[test]
    fn sync_services_are_named_by_their_folder() {
        assert_eq!(
            risks("C:\\Users\\me\\OneDrive\\Backups"),
            [FolderRisk::Synced("OneDrive")]
        );
        assert_eq!(
            risks("C:\\Users\\me\\OneDrive - Contoso Ltd\\Documents"),
            [FolderRisk::Synced("OneDrive")]
        );
        assert_eq!(risks("D:/DropBox/snip"), [FolderRisk::Synced("Dropbox")]);
        assert_eq!(
            risks("G:\\Google Drive\\x"),
            [FolderRisk::Synced("Google Drive")]
        );
        assert_eq!(
            risks("/home/me/iCloud Drive/x"),
            [FolderRisk::Synced("iCloud Drive")]
        );
    }

    #[test]
    fn a_name_that_only_contains_a_service_is_not_one() {
        assert_eq!(risks("C:\\Users\\me\\Dropbox backups old"), []);
        assert_eq!(risks("C:\\Users\\me\\my onedrive notes"), []);
    }
}
