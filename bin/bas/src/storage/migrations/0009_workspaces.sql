CREATE TABLE workspaces (
    id TEXT PRIMARY KEY NOT NULL,
    metadata BLOB NOT NULL
);

-- #[toasty::breakpoint]
PRAGMA user_version = 9;
