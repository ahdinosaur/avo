use comfy_table::Table;
use ludis_machine::Machine;
use ludis_plan::PlanId;
use ludis_system::Hostname;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use thiserror::Error;
use tokio::fs::read_to_string;

#[derive(Error, Debug)]
pub enum MachinesError {
    #[error("machines file not found at: {0}")]
    NotFound(PathBuf),

    #[error("failed to read machines file {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to parse machines file {path}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },

    #[error("failed to resolve plan path: {base_path} + {plan_path}")]
    ResolvingPlanPath {
        base_path: PathBuf,
        plan_path: PathBuf,
    },
}

#[derive(Debug, Clone, Deserialize)]
struct MachinesConfigToml {
    pub machines: BTreeMap<String, MachineConfigToml>,
}

#[derive(Debug, Clone)]
pub struct MachinesConfig {
    pub path: PathBuf,
    pub machines: BTreeMap<String, MachineConfig>,
}

#[derive(Debug, Clone, Deserialize)]
struct MachineConfigToml {
    #[serde(flatten)]
    pub machine: Machine,
    pub plan: PathBuf,
}

#[derive(Debug, Clone)]
pub struct MachineConfig {
    pub machine: Machine,
    pub plan: PlanId,
}

impl MachinesConfig {
    pub async fn load() -> Result<Self, MachinesError> {
        let path = Self::discover_path()?;
        let config = Self::load_config(&path).await?;
        let MachinesConfigToml { machines } = config;
        let machines = machines
            .into_iter()
            .map(|(name, config)| {
                let MachineConfigToml { machine, plan } = config;
                Ok((
                    name,
                    MachineConfig {
                        machine,
                        plan: Self::resolve_plan_id(&path, &plan)?,
                    },
                ))
            })
            .collect::<Result<_, _>>()?;
        Ok(MachinesConfig {
            path: path.to_owned(),
            machines,
        })
    }

    pub fn get(&self, id: &str) -> Option<&MachineConfig> {
        self.machines.get(id)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &MachineConfig)> {
        self.machines.iter()
    }

    pub fn local(&self) -> Option<&MachineConfig> {
        let local_hostname = Hostname::get().ok()?;
        self.machines
            .values()
            .find(|cfg| cfg.machine.hostname == local_hostname)
    }

    pub fn print(&self) {
        let mut table = Table::new();
        table
            .load_preset(comfy_table::presets::UTF8_FULL)
            .apply_modifier(comfy_table::modifiers::UTF8_ROUND_CORNERS)
            .set_content_arrangement(comfy_table::ContentArrangement::Dynamic)
            .set_header(vec!["id", "plan", "hostname", "arch", "os"]);

        for (machine_id, config) in self.iter() {
            let MachineConfig { machine, plan } = config;
            let Machine {
                hostname,
                arch,
                os,
                vm: _,
            } = machine;
            table.add_row(vec![
                machine_id,
                &plan.to_string(),
                &hostname.to_string(),
                &arch.to_string(),
                &os.to_string(),
            ]);
        }

        println!("{table}")
    }

    async fn load_config(path: &Path) -> Result<MachinesConfigToml, MachinesError> {
        let string = read_to_string(path)
            .await
            .map_err(|source| MachinesError::Read {
                path: path.to_owned(),
                source,
            })?;
        let config = toml::from_str(&string).map_err(|source| MachinesError::Parse {
            path: path.to_owned(),
            source,
        })?;
        Ok(config)
    }

    fn discover_path() -> Result<PathBuf, MachinesError> {
        if let Ok(explicit) = std::env::var("LUDIS_MACHINES_PATH") {
            let path = PathBuf::from(explicit);
            if path.exists() {
                return Ok(path);
            }
        }
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let path = cwd.join("machines.toml");
        if path.exists() {
            return Ok(path);
        }
        Err(MachinesError::NotFound(path))
    }

    fn resolve_plan_id(base_path: &Path, plan_path: &Path) -> Result<PlanId, MachinesError> {
        let plan_path = if plan_path.is_absolute() {
            plan_path.to_path_buf()
        } else {
            base_path
                .parent()
                .map(|parent| parent.join(plan_path))
                .ok_or_else(|| MachinesError::ResolvingPlanPath {
                    base_path: base_path.to_owned(),
                    plan_path: plan_path.to_owned(),
                })?
        };
        Ok(PlanId::Path(plan_path))
    }
}
