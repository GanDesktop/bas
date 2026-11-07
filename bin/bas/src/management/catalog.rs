//! Installed and available agents, including the remote registry cache.
use super::*;
use bas_config::agent::Kind;
use bas_protocol::management::{Catalog, Entry, Source, Status};

#[derive(Deserialize, Serialize)]
struct Cache {
    url: String,
    registry: registry::Registry,
}

pub(super) struct RegistrySnapshot {
    pub(super) registry: Option<registry::Registry>,
    pub(super) warning: Option<String>,
}

impl Manager {
    /// # Errors
    /// Reports invalid installation metadata; registry outages are returned as catalog warnings.
    pub async fn catalog(
        &self,
        registrations: Vec<AgentRegistration>,
        refresh: bool,
    ) -> Result<Catalog> {
        let loaded = self
            .load_registry(
                refresh,
                &Progress {
                    sender: None,
                    cancellation: self.stopping.child_token(),
                },
            )
            .await;
        let RegistrySnapshot { registry, warning } = match loaded {
            Ok(loaded) => loaded,
            Err(error) => RegistrySnapshot {
                registry: None,
                warning: Some(format!("ACP 目录暂不可用：{error}")),
            },
        };
        let mut configured: BTreeMap<_, _> = registrations
            .into_iter()
            .map(|registration| (registration.id().as_str().to_owned(), registration))
            .collect();
        let mut entries = Vec::new();
        if let Some(registry) = registry {
            for manifest in registry.agents {
                let registration = configured.remove(&manifest.id);
                let record = registration
                    .as_ref()
                    .and_then(AgentRegistration::managed)
                    .map(installation::load_record)
                    .transpose()?;
                let spec = registration
                    .as_ref()
                    .map(AgentRegistration::resolve)
                    .transpose()?;
                let managed = record.is_some();
                let custom = spec.is_some() && !managed;
                let latest_version = manifest.version.clone();
                let manifest = record
                    .as_ref()
                    .map_or(manifest, |record| record.manifest.clone());
                let (status, message) = if let Some(spec) = &spec {
                    let ready = if managed {
                        record.as_ref().is_some_and(record_available)
                    } else {
                        available(spec)
                    };
                    if ready {
                        (
                            Status::Installed,
                            if custom {
                                "自定义配置".into()
                            } else {
                                "安装完成".into()
                            },
                        )
                    } else {
                        (
                            Status::Missing,
                            if managed {
                                "启动程序不存在；请重新安装".into()
                            } else {
                                "启动程序不存在；请检查自定义配置".into()
                            },
                        )
                    }
                } else {
                    match manifest.select() {
                        Ok(_) => (Status::NotInstalled, String::new()),
                        Err(error) => (Status::Unsupported, error.to_string()),
                    }
                };
                entries.push(Entry {
                    id: manifest.id.clone(),
                    name: if custom {
                        manifest.id.clone()
                    } else {
                        manifest.name
                    },
                    description: if custom {
                        spec.as_ref()
                            .map_or_else(String::new, |spec| spec.description.clone())
                    } else {
                        manifest.description
                    },
                    version: latest_version,
                    repository: manifest.repository,
                    license: manifest.license,
                    license_url: manifest.license_url,
                    icon: manifest.icon,
                    installed_version: record
                        .as_ref()
                        .map(|record| record.manifest.version.clone()),
                    source: if custom
                        && spec
                            .as_ref()
                            .is_some_and(|spec| spec.kind == Kind::Embedded)
                    {
                        Source::Embedded
                    } else if custom {
                        Source::Custom
                    } else {
                        Source::Registry
                    },
                    status,
                    message,
                    spec,
                });
            }
        }
        for (id, registration) in configured {
            let record = registration
                .managed()
                .map(installation::load_record)
                .transpose()?;
            let record = record.as_ref();
            let spec = registration.resolve()?;
            let source = if record.is_some() {
                Source::Registry
            } else if spec.kind == Kind::Embedded {
                Source::Embedded
            } else {
                Source::Custom
            };
            entries.push(Entry {
                name: record.map_or_else(|| id.clone(), |record| record.manifest.name.clone()),
                id,
                description: record.map_or_else(
                    || spec.description.clone(),
                    |record| record.manifest.description.clone(),
                ),
                version: String::new(),
                repository: record
                    .map_or_else(String::new, |record| record.manifest.repository.clone()),
                license: record.map_or_else(String::new, |record| record.manifest.license.clone()),
                license_url: record
                    .map_or_else(String::new, |record| record.manifest.license_url.clone()),
                icon: record.map_or_else(String::new, |record| record.manifest.icon.clone()),
                installed_version: record.map(|record| record.manifest.version.clone()),
                source,
                status: if record.map_or_else(|| available(&spec), record_available) {
                    Status::Installed
                } else {
                    Status::Missing
                },
                message: if record.map_or_else(|| available(&spec), record_available) {
                    String::new()
                } else if record.is_some() {
                    "启动程序不存在；请重新安装".into()
                } else {
                    "启动程序不存在；请检查自定义配置".into()
                },
                spec: Some(spec),
            });
        }
        entries.sort_by_key(|entry| entry.name.to_lowercase());
        Ok(Catalog {
            entries,
            warning,
            settings_path: self
                .store
                .as_ref()
                .map(|store| std::path::absolute(store.path()))
                .transpose()?,
            can_install: self.store.is_some(),
        })
    }

    pub(super) async fn load_registry(
        &self,
        refresh: bool,
        progress: &Progress,
    ) -> Result<RegistrySnapshot> {
        let mut memory = tokio::select! {
            biased;
            _ = progress.closed() => return Err(Error::Cancelled.into()),
            memory = self.registry.lock() => memory,
        };
        progress.check()?;
        let cache_path = self.root.join("registry.json");
        if memory.registry.is_none() {
            artifact::reject_file_link(&cache_path)?;
            if let Ok(bytes) = fs::read(&cache_path)
                && let Ok(cache) = serde_json::from_slice::<Cache>(&bytes)
                && cache.url == self.url
                && cache.registry.validate().is_ok()
            {
                memory.registry = Some(cache.registry);
            }
        }
        if !refresh && memory.registry.is_some() {
            return Ok(RegistrySnapshot {
                registry: memory.registry.clone(),
                warning: memory.warning.clone(),
            });
        }
        let downloaded: Result<registry::Registry> = async {
            let registry: registry::Registry =
                runtime::get_json(&self.client, &self.url, progress).await?;
            registry.validate()?;
            Ok(registry)
        }
        .await;
        match downloaded {
            Ok(registry) => {
                let warning = artifact::atomic_json(
                    &cache_path,
                    &Cache {
                        url: self.url.clone(),
                        registry: registry.clone(),
                    },
                )
                .err()
                .map(|error| format!("ACP 目录缓存保存失败：{error}"));
                memory.registry = Some(registry.clone());
                memory.warning = warning.clone();
                Ok(RegistrySnapshot {
                    registry: Some(registry),
                    warning,
                })
            }
            Err(error) if memory.registry.is_some() => {
                progress.check()?;
                memory.warning = Some(format!("ACP 目录刷新失败，正在使用缓存：{error}"));
                Ok(RegistrySnapshot {
                    registry: memory.registry.clone(),
                    warning: memory.warning.clone(),
                })
            }
            Err(error) => Err(error),
        }
    }
}

fn available(spec: &AgentSpec) -> bool {
    if spec.kind == Kind::Embedded {
        return true;
    }
    crate::agent::acp::resolve_program(spec).is_some()
}

fn record_available(record: &Record) -> bool {
    if !available(&record.spec) || !record.directory.is_dir() {
        return false;
    }
    match record.manifest.select() {
        Ok(registry::Selected::Binary(binary)) => registry::safe_relative(&binary.cmd)
            .is_ok_and(|path| record.directory.join(path).is_file()),
        Ok(registry::Selected::Npm(_)) => {
            let dependencies_ready = record.dependencies.iter().all(|(name, version)| {
                fs::read(
                    record
                        .directory
                        .join("node_modules")
                        .join(name)
                        .join("package.json"),
                )
                .ok()
                .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
                .is_some_and(|package| {
                    package.get("version").and_then(|version| version.as_str())
                        == Some(version.as_str())
                })
            });
            let command_ready = record.spec.command().is_some_and(|command| {
                command.program.starts_with(&record.directory)
                    || command.args.first().is_some_and(|arg| {
                        let path = PathBuf::from(arg);
                        path.starts_with(&record.directory) && path.is_file()
                    })
            });
            dependencies_ready && command_ready
        }
        Err(_) => false,
    }
}
