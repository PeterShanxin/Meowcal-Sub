use std::path::{Path, PathBuf};

/// Consumers Core accepts. Each owns `<base>/<client>/<profile>/<version>/<architecture>`.
pub const CLIENTS: [&str; 2] = ["sub1", "sub2"];

/// Other install partitions for a root's profile and architecture, newest first.
#[derive(Debug, Default)]
pub struct SiblingPartitions {
    /// This client's other Core versions.
    pub own: Vec<PathBuf>,
    /// Other clients' partitions, then `<base>/<profile>/<version>/<architecture>`,
    /// the unpartitioned layout Core 0.1.0 through 0.1.3 wrote.
    pub shared: Vec<PathBuf>,
}

pub fn sibling_partitions(root: &Path) -> SiblingPartitions {
    let parts = (|| {
        let architecture = root.file_name()?;
        let version_dir = root.parent()?;
        let profile_dir = version_dir.parent()?;
        let client_dir = profile_dir.parent()?;
        let client = client_dir.file_name()?.to_str()?;
        CLIENTS.contains(&client).then_some((
            architecture,
            version_dir.file_name()?,
            profile_dir,
            profile_dir.file_name()?,
            client,
            client_dir.parent()?,
        ))
    })();
    let Some((architecture, current, profile_dir, profile, client, base)) = parts else {
        return SiblingPartitions::default();
    };
    let mut shared: Vec<PathBuf> = CLIENTS
        .iter()
        .filter(|other| **other != client)
        .flat_map(|other| versions(&base.join(other).join(profile), architecture, None))
        .collect();
    shared.extend(versions(&base.join(profile), architecture, None));
    SiblingPartitions {
        own: versions(profile_dir, architecture, Some(current)),
        shared,
    }
}

fn versions(
    profile_dir: &Path,
    architecture: &std::ffi::OsStr,
    exclude: Option<&std::ffi::OsStr>,
) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(profile_dir) else {
        return Vec::new();
    };
    let mut partitions: Vec<(Vec<u64>, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter(|entry| Some(entry.file_name().as_os_str()) != exclude)
        .filter_map(|entry| {
            let version = entry
                .file_name()
                .to_str()?
                .split('.')
                .map(|part| part.parse::<u64>().ok())
                .collect::<Option<Vec<_>>>()?;
            let partition = entry.path().join(architecture);
            (version.len() == 3 && partition.is_dir()).then_some((version, partition))
        })
        .collect();
    partitions.sort_by(|left, right| right.0.cmp(&left.0));
    partitions
        .into_iter()
        .map(|(_, partition)| partition)
        .collect()
}

#[cfg(test)]
#[path = "storage_partitions_tests.rs"]
mod tests;
