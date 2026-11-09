use bas_client::service::Client;

#[derive(Clone, Default)]
pub(crate) enum Backend {
    #[default]
    Disconnected,
    Connecting,
    Connected {
        client: Client,
        endpoint: String,
    },
    Failed(String),
}

impl Backend {
    pub fn connected(&self) -> bool {
        matches!(self, Self::Connected { .. })
    }
    pub fn connecting(&self) -> bool {
        matches!(self, Self::Connecting)
    }
    pub fn client(&self) -> Option<Client> {
        match self {
            Self::Connected { client, .. } => Some(client.clone()),
            _ => None,
        }
    }
    pub fn endpoint(&self) -> Option<&str> {
        match self {
            Self::Connected { endpoint, .. } => Some(endpoint),
            _ => None,
        }
    }
    pub fn error(&self) -> &str {
        match self {
            Self::Failed(error) => error,
            _ => "",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CreateSession {
    pub agent: String,
    pub workspace: String,
    pub generation: u64,
    pub selection: u64,
}

#[derive(Clone, Default)]
pub(crate) enum Creation {
    #[default]
    Idle,
    Creating(CreateSession),
    AuthenticationRequired(CreateSession),
    Failed {
        request: CreateSession,
        error: String,
    },
}

impl Creation {
    pub fn creating(&self) -> bool {
        matches!(self, Self::Creating(_))
    }
}

#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) enum Execution {
    #[default]
    Idle,
    Submitting,
    Running,
    Uncertain,
}

impl Execution {
    pub fn running(&self) -> bool {
        *self == Self::Running
    }
    pub fn submitting(&self) -> bool {
        *self == Self::Submitting
    }
    pub fn can_send(&self) -> bool {
        *self == Self::Idle
    }
    pub fn observe(&mut self, active: bool) {
        if matches!(self, Self::Idle | Self::Running) {
            *self = if active { Self::Running } else { Self::Idle };
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum LoginTarget {
    Create(CreateSession),
    Session { id: String, agent: String },
}

impl LoginTarget {
    pub fn agent(&self) -> &str {
        match self {
            Self::Create(request) => &request.agent,
            Self::Session { agent, .. } => agent,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LoginRequest {
    pub target: LoginTarget,
    pub generation: u64,
    pub selection: u64,
}
