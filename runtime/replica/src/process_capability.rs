//! Explicit process-lane capabilities owned by Runtime configuration.

use std::{collections::BTreeMap, path::PathBuf};

/// One configured executable selected by its exact argv[0] alias.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessExecutable {
    alias: String,
    path: PathBuf,
}

impl ProcessExecutable {
    /// Freezes a non-empty alias and absolute executable path.
    pub fn new(alias: impl Into<String>, path: impl Into<PathBuf>) -> Result<Self, String> {
        let value = Self {
            alias: alias.into(),
            path: path.into(),
        };
        if value.alias.is_empty()
            || value.alias.contains('/')
            || value.alias.as_bytes().contains(&0)
            || !value.path.is_absolute()
        {
            return Err("invalid process executable capability".into());
        }
        Ok(value)
    }

    /// Returns the exact argv[0] alias exposed to the tool contract.
    pub fn alias(&self) -> &str {
        &self.alias
    }

    /// Returns the configured absolute executable path.
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}

/// One named, immutable process capability supplied to Runtime construction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessLane {
    name: String,
    executables: BTreeMap<String, ProcessExecutable>,
    environment: BTreeMap<String, String>,
}

impl ProcessLane {
    /// Freezes exact executables and environment values without reading the host environment.
    pub fn new(
        name: impl Into<String>,
        executables: impl IntoIterator<Item = ProcessExecutable>,
        environment: impl IntoIterator<Item = (String, String)>,
    ) -> Result<Self, String> {
        let name = name.into();
        if !valid_lane_name(&name) {
            return Err("invalid process lane name".into());
        }
        let mut executable_map = BTreeMap::new();
        for executable in executables {
            if executable_map
                .insert(executable.alias.clone(), executable)
                .is_some()
            {
                return Err("duplicate process executable alias".into());
            }
        }
        if executable_map.is_empty() {
            return Err("empty process executable set".into());
        }
        let mut environment_map = BTreeMap::new();
        for (key, value) in environment {
            if !valid_environment_key(&key)
                || value.as_bytes().contains(&0)
                || environment_map.insert(key, value).is_some()
            {
                return Err("invalid process environment capability".into());
            }
        }
        Ok(Self {
            name,
            executables: executable_map,
            environment: environment_map,
        })
    }

    /// Returns the exact lane identity admitted by the T1 catalogue.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Resolves argv[0] without consulting PATH.
    pub fn executable(&self, alias: &str) -> Option<&ProcessExecutable> {
        self.executables.get(alias)
    }

    /// Returns the complete environment to install after `env_clear`.
    pub fn environment(&self) -> &BTreeMap<String, String> {
        &self.environment
    }
}

/// Frozen process lanes used by both the catalogue and concrete executor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessLaneRegistry {
    lanes: BTreeMap<String, ProcessLane>,
}

impl ProcessLaneRegistry {
    /// Constructs a non-empty registry and rejects duplicate lane identities.
    pub fn new(lanes: impl IntoIterator<Item = ProcessLane>) -> Result<Self, String> {
        let mut values = BTreeMap::new();
        for lane in lanes {
            if values.insert(lane.name.clone(), lane).is_some() {
                return Err("duplicate process lane".into());
            }
        }
        if values.is_empty() {
            return Err("empty process lane registry".into());
        }
        Ok(Self { lanes: values })
    }

    /// Resolves one exact lane identity.
    pub fn lane(&self, name: &str) -> Option<&ProcessLane> {
        self.lanes.get(name)
    }

    /// Returns exact lane identities for T1 catalogue construction.
    pub fn lane_names(&self) -> impl Iterator<Item = &str> {
        self.lanes.keys().map(String::as_str)
    }
}

fn valid_lane_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b':' | b'/')
        })
}

fn valid_environment_key(value: &str) -> bool {
    let mut bytes = value.bytes();
    bytes
        .next()
        .is_some_and(|byte| byte == b'_' || byte.is_ascii_alphabetic())
        && bytes.all(|byte| byte == b'_' || byte.is_ascii_alphanumeric())
}
