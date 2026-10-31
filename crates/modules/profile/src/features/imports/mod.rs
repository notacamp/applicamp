//! Importing a profile: a LinkedIn data export ZIP, a PDF (a LinkedIn
//! profile or a CV) or pasted text becomes a **draft** the person reviews.
//! Nothing reaches their profile until they apply it (Replace or Merge);
//! the uploaded file is read in memory and never stored, only the
//! structured draft, which expires after [`DRAFT_LIFETIME`].

pub mod linkedin;
pub mod pdf;
pub(crate) mod source_check;
pub mod structure;

#[cfg(test)]
pub(crate) mod testkit;
#[cfg(test)]
mod tests;

use applicamp_ai::{Caller, Gateway, Role};
use applicamp_kernel::{
    AppState, Error, Job, JobContext, JobError, Problem, Result, auth::SignedIn, jobs,
};
use axum::{
    Extension, Json,
    extract::{Multipart, Path, State, rejection::JsonRejection},
    http::StatusCode,
};
use chrono::{DateTime, TimeDelta, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    events::ProfileUpdated,
    features::profile::{ProfileView, view_of},
    schema::Profile,
};

/// How long a draft waits to be applied.
pub const DRAFT_LIFETIME: TimeDelta = TimeDelta::hours(24);
/// Largest LinkedIn export accepted.
pub const MAX_ZIP_BYTES: usize = 20 * 1024 * 1024;
/// Largest PDF accepted.
pub const MAX_PDF_BYTES: usize = 8 * 1024 * 1024;
/// Longest text accepted, pasted or read from a PDF, in characters.
pub const MAX_TEXT_CHARS: usize = 60_000;
/// The request body limit of an upload: the largest file and the form
/// around it.
pub const UPLOAD_BODY_LIMIT: usize = MAX_ZIP_BYTES + 64 * 1024;

/// The AI gateway the module was composed with, if any.
#[derive(Debug, Clone, Default)]
pub struct Ai(pub Option<Gateway>);

impl Ai {
    /// Whether the `complete` role can structure a PDF or text.
    pub fn can_structure(&self) -> bool {
        self.0
            .as_ref()
            .is_some_and(|g| g.is_configured(Role::Complete))
    }
}

/// What an import is made from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ImportKind {
    /// The LinkedIn data export, a ZIP of CSV files. Needs no model.
    LinkedinZip,
    /// A PDF: a LinkedIn profile saved as PDF, or a CV. Needs the model.
    Pdf,
    /// Pasted text: a CV or a profile. Needs the model.
    Text,
}

impl ImportKind {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::LinkedinZip => "linkedin_zip",
            Self::Pdf => "pdf",
            Self::Text => "text",
        }
    }

    pub(crate) fn parse(raw: &str) -> Option<Self> {
        match raw.trim() {
            "linkedin_zip" => Some(Self::LinkedinZip),
            "pdf" => Some(Self::Pdf),
            "text" => Some(Self::Text),
            _ => None,
        }
    }
}

/// Where an import is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ImportStatus {
    /// Waiting to be reviewed.
    Ready,
    /// Applied to the profile.
    Applied,
    /// Thrown away.
    Discarded,
}

impl ImportStatus {
    fn parse(raw: &str) -> Self {
        match raw {
            "applied" => Self::Applied,
            "discarded" => Self::Discarded,
            _ => Self::Ready,
        }
    }
}

/// A draft profile waiting for review.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Import {
    /// The import.
    pub id: Uuid,
    /// What it was made from.
    pub kind: ImportKind,
    /// Where it is.
    pub status: ImportStatus,
    /// The draft.
    pub draft: Profile,
    /// Values left out of the draft: ones a model gave that the file does
    /// not say (the source check), or that did not fit.
    pub dropped: u32,
    /// When it was made.
    pub created_at: DateTime<Utc>,
    /// When the draft is deleted if it is not applied.
    pub expires_at: DateTime<Utc>,
}

/// An upload (`multipart/form-data`): the kind, then the file (ZIP or PDF)
/// or the text.
#[derive(Debug, Default, ToSchema)]
pub struct ImportUpload {
    /// `linkedin_zip`, `pdf` or `text`.
    #[schema(value_type = ImportKind)]
    pub kind: Option<String>,
    /// The file, for `linkedin_zip` (up to 20 MB) and `pdf` (up to 8 MB).
    #[schema(value_type = Option<String>, format = Binary)]
    pub file: Option<Vec<u8>>,
    /// The text, for `text` (up to 60 000 characters).
    pub text: Option<String>,
}

fn upload_error(detail: &str) -> Error {
    Error::BadRequest("invalid_upload", detail.to_owned())
}

impl ImportUpload {
    /// Read the form, never holding more than an upload's limit.
    async fn read(mut form: Multipart) -> Result<Self> {
        let mut upload = Self::default();
        while let Some(mut field) = form
            .next_field()
            .await
            .map_err(|_| upload_error("The upload could not be read."))?
        {
            let name = field.name().unwrap_or_default().to_owned();
            match name.as_str() {
                "kind" => {
                    upload.kind = Some(
                        field
                            .text()
                            .await
                            .map_err(|_| upload_error("The upload could not be read."))?,
                    );
                }
                "file" => {
                    let mut bytes = Vec::new();
                    while let Some(chunk) = field
                        .chunk()
                        .await
                        .map_err(|_| too_large("This file is over 20 MB."))?
                    {
                        if bytes.len() + chunk.len() > MAX_ZIP_BYTES {
                            return Err(too_large("This file is over 20 MB."));
                        }
                        bytes.extend_from_slice(&chunk);
                    }
                    upload.file = Some(bytes);
                }
                "text" => {
                    let mut bytes = Vec::new();
                    while let Some(chunk) = field
                        .chunk()
                        .await
                        .map_err(|_| upload_error("The upload could not be read."))?
                    {
                        // Four bytes per character at most.
                        if bytes.len() + chunk.len() > MAX_TEXT_CHARS * 4 {
                            return Err(too_long());
                        }
                        bytes.extend_from_slice(&chunk);
                    }
                    upload.text = Some(
                        String::from_utf8(bytes)
                            .map_err(|_| upload_error("The text is not UTF-8."))?,
                    );
                }
                _ => {}
            }
        }
        Ok(upload)
    }
}

fn too_large(detail: &str) -> Error {
    Error::BadRequest("file_too_large", detail.to_owned())
}

fn too_long() -> Error {
    Error::BadRequest(
        "text_too_long",
        format!("The text is over {MAX_TEXT_CHARS} characters."),
    )
}

fn checked_text(text: String) -> Result<String> {
    if text.chars().count() > MAX_TEXT_CHARS {
        return Err(too_long());
    }
    if text.trim().is_empty() {
        return Err(upload_error("There is no text to import."));
    }
    Ok(text)
}

/// Import a profile.
///
/// A `multipart/form-data` upload of `kind` and either `file` (a LinkedIn
/// export ZIP, up to 20 MB; a PDF, up to 8 MB) or `text` (up to 60 000
/// characters). The answer is a draft to review: the profile is unchanged
/// until the draft is applied. The file is not kept.
///
/// A ZIP is read from its CSVs without any model; it is refused before
/// anything is extracted when it holds more than 200 files, a file over
/// 5 MB or more than 50 MB uncompressed, or a path leaving the archive.
/// A PDF's text and pasted text are structured by the `complete` model, and
/// every value the source does not say is dropped and counted (`dropped`).
#[utoipa::path(
    post,
    path = "/api/v1/profile/imports",
    tag = "profile",
    operation_id = "profile.import",
    security(("session" = [])),
    request_body(content = ImportUpload, content_type = "multipart/form-data"),
    responses(
        (status = 201, description = "The draft, waiting for review.", body = Import),
        (status = 400, description = "`invalid_upload`, `invalid_kind`, `file_too_large`, \
            `text_too_long`, `not_a_zip`, `archive_too_many_entries`, \
            `archive_entry_too_large`, `archive_too_large`, `archive_unsafe_path`, \
            `not_a_pdf`, `pdf_unreadable`, `pdf_too_many_pages`, `pdf_no_text`, \
            `nothing_found`", body = Problem, content_type = "application/problem+json"),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 503, description = "`ai_unavailable`: no model to read a PDF or text \
            (import the LinkedIn ZIP instead); `ai_daily_limit_reached`; `ai_failed`",
            body = Problem, content_type = "application/problem+json"),
    ),
)]
pub async fn create(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(ai): Extension<Ai>,
    form: Multipart,
) -> Result<(StatusCode, Json<Import>)> {
    let upload = ImportUpload::read(form).await?;
    let kind = upload
        .kind
        .as_deref()
        .and_then(ImportKind::parse)
        .ok_or_else(|| {
            Error::BadRequest(
                "invalid_kind",
                "Say what you are importing: linkedin_zip, pdf or text.".to_owned(),
            )
        })?;
    if kind != ImportKind::LinkedinZip && !ai.can_structure() {
        return Err(Error::Unavailable(applicamp_ai::AI_UNAVAILABLE));
    }
    let now = state.clock.now();
    let caller = Caller {
        db: &state.db,
        account: principal.account_id,
        now,
    };
    let (draft, dropped) = match kind {
        ImportKind::LinkedinZip => {
            let bytes = upload
                .file
                .ok_or_else(|| upload_error("Choose the ZIP LinkedIn sent you."))?;
            let draft = tokio::task::spawn_blocking(move || linkedin::read_export(&bytes))
                .await
                .map_err(Error::internal)??;
            (draft, 0)
        }
        ImportKind::Pdf => {
            let bytes = upload.file.ok_or_else(|| upload_error("Choose a PDF."))?;
            if bytes.len() > MAX_PDF_BYTES {
                return Err(too_large("This PDF is over 8 MB."));
            }
            let text = checked_text(pdf::text_of(bytes).await?)?;
            structure::structure(ai.0.as_ref(), caller, &text).await?
        }
        ImportKind::Text => {
            let text = checked_text(upload.text.unwrap_or_default())?;
            structure::structure(ai.0.as_ref(), caller, &text).await?
        }
    };
    let (draft, unfit) = draft.sanitized();
    if draft.is_empty() {
        return Err(Error::BadRequest(
            "nothing_found",
            "We found nothing to import in this. Check it is the right file.".to_owned(),
        ));
    }
    let import = Import {
        id: Uuid::now_v7(),
        kind,
        status: ImportStatus::Ready,
        draft,
        dropped: dropped.saturating_add(unfit),
        created_at: now,
        expires_at: now + DRAFT_LIFETIME,
    };
    let mut tx = state.db.begin().await?;
    sqlx::query(
        "INSERT INTO profile.imports (id, account_id, kind, status, draft, dropped, created_at, \
         expires_at) VALUES ($1, $2, $3, 'ready', $4, $5, $6, $7)",
    )
    .bind(import.id)
    .bind(principal.account_id)
    .bind(kind.as_str())
    .bind(sqlx::types::Json(&import.draft))
    .bind(i32::try_from(import.dropped).unwrap_or(i32::MAX))
    .bind(import.created_at)
    .bind(import.expires_at)
    .execute(&mut *tx)
    .await?;
    jobs::enqueue_at(&mut tx, &ExpireImports {}, import.expires_at).await?;
    tx.commit().await?;
    // Ids, kinds and counts only.
    tracing::info!(import = %import.id, kind = kind.as_str(), dropped = import.dropped, "profile import drafted");
    Ok((StatusCode::CREATED, Json(import)))
}

#[derive(sqlx::FromRow)]
struct ImportRow {
    id: Uuid,
    kind: String,
    status: String,
    draft: sqlx::types::Json<Profile>,
    dropped: i32,
    created_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
}

impl ImportRow {
    fn into_import(self) -> Result<Import> {
        Ok(Import {
            id: self.id,
            kind: ImportKind::parse(&self.kind)
                .ok_or_else(|| Error::internal("an import of an unknown kind"))?,
            status: ImportStatus::parse(&self.status),
            draft: self.draft.0,
            dropped: u32::try_from(self.dropped).unwrap_or(0),
            created_at: self.created_at,
            expires_at: self.expires_at,
        })
    }
}

/// A draft.
///
/// One of the signed-in person's imports, until it expires. Anyone else's
/// is not found.
#[utoipa::path(
    get,
    path = "/api/v1/profile/imports/{id}",
    tag = "profile",
    operation_id = "profile.get_import",
    security(("session" = [])),
    params(("id" = Uuid, Path, description = "The import.")),
    responses(
        (status = 200, description = "The import and its draft.", body = Import),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn get(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Path(id): Path<Uuid>,
) -> Result<Json<Import>> {
    let row: ImportRow = sqlx::query_as(
        "SELECT id, kind, status, draft, dropped, created_at, expires_at FROM profile.imports
          WHERE id = $1 AND account_id = $2 AND expires_at > $3 AND draft IS NOT NULL",
    )
    .bind(id)
    .bind(principal.account_id)
    .bind(state.clock.now())
    .fetch_optional(&state.db)
    .await?
    .ok_or(Error::NotFound)?;
    Ok(Json(row.into_import()?))
}

/// How a draft reaches the profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ApplyMode {
    /// The draft becomes the profile.
    Replace,
    /// The profile keeps everything it has, fills its empty fields from the
    /// draft and gains the draft's positions, schools, skills, languages,
    /// certifications and projects it does not have yet.
    Merge,
}

/// How to apply a draft.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ApplyImport {
    /// Replace the profile, or merge into it.
    pub mode: ApplyMode,
}

fn not_ready() -> Error {
    Error::Conflict(
        "import_not_ready",
        "This import was already applied or discarded.".to_owned(),
    )
}

/// Apply a draft.
///
/// `replace` makes the draft the profile; `merge` keeps the profile as it
/// is (hand edits included) and adds what the draft has that it does not.
/// Once applied, a draft cannot be applied again.
#[utoipa::path(
    post,
    path = "/api/v1/profile/imports/{id}/apply",
    tag = "profile",
    operation_id = "profile.apply_import",
    security(("session" = [])),
    params(("id" = Uuid, Path, description = "The import.")),
    request_body = ApplyImport,
    responses(
        (status = 200, description = "The profile, now.", body = ProfileView),
        (status = 400, description = "`invalid_request`", body = Problem,
            content_type = "application/problem+json"),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`", body = Problem,
            content_type = "application/problem+json"),
        (status = 409, description = "`import_not_ready`: already applied or discarded",
            body = Problem, content_type = "application/problem+json"),
    ),
)]
pub async fn apply(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(ai): Extension<Ai>,
    Path(id): Path<Uuid>,
    body: std::result::Result<Json<ApplyImport>, JsonRejection>,
) -> Result<Json<ProfileView>> {
    let Json(ApplyImport { mode }) =
        body.map_err(|rejection| Error::BadRequest("invalid_request", rejection.body_text()))?;
    let now = state.clock.now();
    let mut tx = state.db.begin().await?;
    let row: ImportRow = sqlx::query_as(
        "SELECT id, kind, status, draft, dropped, created_at, expires_at FROM profile.imports
          WHERE id = $1 AND account_id = $2 AND expires_at > $3 AND draft IS NOT NULL
            FOR UPDATE",
    )
    .bind(id)
    .bind(principal.account_id)
    .bind(now)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(Error::NotFound)?;
    let import = row.into_import()?;
    if import.status != ImportStatus::Ready {
        return Err(not_ready());
    }
    let current: Option<sqlx::types::Json<Profile>> =
        sqlx::query_scalar("SELECT data FROM profile.profiles WHERE account_id = $1 FOR UPDATE")
            .bind(principal.account_id)
            .fetch_optional(&mut *tx)
            .await?;
    let profile = match mode {
        ApplyMode::Replace => import.draft,
        ApplyMode::Merge => current
            .map(|c| c.0)
            .unwrap_or_default()
            .merged_with(import.draft),
    };
    sqlx::query(
        "INSERT INTO profile.profiles (account_id, data, source, updated_at)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (account_id) DO UPDATE
            SET data = excluded.data, source = excluded.source, updated_at = excluded.updated_at",
    )
    .bind(principal.account_id)
    .bind(sqlx::types::Json(&profile))
    .bind(import.kind.as_str())
    .bind(now)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE profile.imports SET status = 'applied' WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    state
        .events
        .publish(
            &mut tx,
            &ProfileUpdated {
                account: principal.account_id,
            },
        )
        .await?;
    tx.commit().await?;
    tracing::info!(import = %id, mode = ?mode, "profile import applied");
    Ok(Json(view_of(
        profile,
        Some(import.kind.as_str()),
        Some(now),
        &ai,
    )))
}

/// Throw a draft away.
///
/// The profile is unchanged. Discarding twice is fine; an applied draft
/// cannot be discarded.
#[utoipa::path(
    post,
    path = "/api/v1/profile/imports/{id}/discard",
    tag = "profile",
    operation_id = "profile.discard_import",
    security(("session" = [])),
    params(("id" = Uuid, Path, description = "The import.")),
    responses(
        (status = 204, description = "Discarded."),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`", body = Problem,
            content_type = "application/problem+json"),
        (status = 409, description = "`import_not_ready`: already applied", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn discard(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Path(id): Path<Uuid>,
) -> Result<StatusCode> {
    let status: Option<String> = sqlx::query_scalar(
        "SELECT status FROM profile.imports
          WHERE id = $1 AND account_id = $2 AND expires_at > $3",
    )
    .bind(id)
    .bind(principal.account_id)
    .bind(state.clock.now())
    .fetch_optional(&state.db)
    .await?;
    match status.as_deref() {
        None => Err(Error::NotFound),
        Some("applied") => Err(not_ready()),
        Some(_) => {
            sqlx::query(
                "UPDATE profile.imports SET status = 'discarded'
                  WHERE id = $1 AND status <> 'applied'",
            )
            .bind(id)
            .execute(&state.db)
            .await?;
            Ok(StatusCode::NO_CONTENT)
        }
    }
}

/// Delete the drafts whose time is up. Enqueued with every import, to run
/// when it expires; idempotent.
#[derive(Debug, Serialize, Deserialize)]
pub struct ExpireImports {}

impl Job for ExpireImports {
    const NAME: &'static str = "profile.expire_imports";

    async fn run(self, ctx: JobContext) -> std::result::Result<(), JobError> {
        let mut deleted = sqlx::query("DELETE FROM profile.imports WHERE expires_at <= now()")
            .execute(&ctx.db)
            .await?
            .rows_affected();
        // The drafts made from answers expire the same way.
        deleted += sqlx::query("DELETE FROM profile.drafts WHERE expires_at <= now()")
            .execute(&ctx.db)
            .await?
            .rows_affected();
        // So do the facts "Grill me" took from an answer and nobody kept.
        deleted += sqlx::query("DELETE FROM profile.grill_proposals WHERE expires_at <= now()")
            .execute(&ctx.db)
            .await?
            .rows_affected();
        // And what "Tell me about you" kept for a week.
        deleted += sqlx::query("DELETE FROM profile.about_you WHERE expires_at <= now()")
            .execute(&ctx.db)
            .await?
            .rows_affected();
        if deleted > 0 {
            tracing::info!(deleted, "expired profile imports and drafts deleted");
        }
        Ok(())
    }
}
