//! Every CV draft is a PDF you can see (#73).
//!
//! As soon as a draft is made, and again after every change to it, its text
//! is rendered as a PDF inside the server, the same way an approved version
//! is: Clean on one page, else Clean on two, else Compact on two (the first
//! that fits; after a change, the layout the person chose comes first). What
//! the read-back finds is kept as a note for the person: a preview is never
//! refused for it (Approve still is). A render that fails keeps a short code
//! and is logged with its cause, never with the CV's text.
//!
//! The person can ask for another layout (`POST …/drafts/{n}/preview`), with
//! no model call, and see or download the preview
//! (`GET …/drafts/{n}/preview.pdf`). Approving reuses the preview's PDF when
//! the draft has not changed since and the layout is the same.

use applicamp_kernel::{AppState, Error, Problem, Result, auth::SignedIn};
use axum::{
    Extension, Json,
    extract::{Path, Query, State, rejection::JsonRejection},
    http::{HeaderValue, StatusCode, header},
    response::Response,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use super::{
    Cv, Opp, Row, build, json_body,
    model::{Content, Frame},
    open_draft,
    pdf::{self, Length, Made, Refusal, Template, readback::PdfChecks},
    versions::{LayoutRequest, PdfFile, disposition, file_stem, keywords_on_cv, layout},
    wants_of,
};
use crate::PipelineDeps;

/// What the preview of a draft is, as the review page shows it.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CvPreview {
    /// There is a PDF to show (`…/preview.pdf`).
    pub ready: bool,
    /// The layout it was made with (or tried, when it failed).
    pub template: Option<Template>,
    /// The most pages it may take: 1 or 2.
    pub length: Option<u8>,
    /// The pages it takes.
    pub pages: Option<u32>,
    /// What the read-back found: a note, never a refusal.
    pub checks: Option<PdfChecks>,
    /// Why the last render failed: `cv_doesnt_fit` or `cv_pdf_unavailable`.
    pub error: Option<String>,
    /// When it was made.
    pub at: Option<DateTime<Utc>>,
}

/// The layouts tried, in order, starting from `start`: it, the same
/// template on two pages, then Compact on two.
pub(super) fn ladder(start: (Template, Length)) -> Vec<(Template, Length)> {
    let mut out = vec![start];
    for next in [(start.0, Length::Two), (Template::Compact, Length::Two)] {
        if !out.contains(&next) {
            out.push(next);
        }
    }
    out
}

/// The layout a row's preview has, when it has one.
pub(super) fn layout_of(row: &Row) -> Option<(Template, Length)> {
    Some((
        Template::parse(row.preview_template.as_deref()?)?,
        Length::from_pages(u8::try_from(row.preview_length?).ok()?)?,
    ))
}

/// The preview of `row`, as the page shows it.
pub(super) fn view(row: &Row) -> Option<CvPreview> {
    let at = row.preview_at?;
    Some(CvPreview {
        ready: row.preview_ready,
        template: row.preview_template.as_deref().and_then(Template::parse),
        length: row.preview_length.and_then(|l| u8::try_from(l).ok()),
        pages: row.preview_pages.and_then(|p| u32::try_from(p).ok()),
        checks: row.preview_checks.as_ref().map(|c| c.0.clone()),
        error: row.preview_error.clone(),
        at: Some(at),
    })
}

/// The PDF of a draft's `content` in one layout, or why not. `strict`
/// refuses a PDF the read-back finds lacking (what Approve needs); else it
/// is made with what the read-back found. The one way a CV becomes a PDF.
#[allow(clippy::too_many_arguments)]
pub(super) async fn render_draft(
    db: &PgPool,
    opp: &Opp,
    content: &Content,
    frame: &Frame,
    language: &str,
    template: Template,
    length: Length,
    strict: bool,
) -> Result<std::result::Result<Made, Refusal>> {
    let wants = wants_of(db, opp.id).await?;
    let keywords = keywords_on_cv(&wants, opp, content, frame);
    Ok(if strict {
        pdf::make(content, frame, language, template, length, keywords).await
    } else {
        pdf::preview(content, frame, language, template, length, keywords).await
    })
}

/// What the PDF of `content` is made from, as a fingerprint: Approve keeps
/// a preview only when the draft still says exactly this.
fn fingerprint(content: &Content) -> Vec<u8> {
    applicamp_kernel::crypto::hash(&serde_json::to_string(content).unwrap_or_default())
}

/// What a render that failed is called on the page.
const fn error_code(refusal: &Refusal) -> &'static str {
    match refusal {
        Refusal::DoesntFit { .. } => "cv_doesnt_fit",
        Refusal::Checks(_) | Refusal::Unavailable(_) => "cv_pdf_unavailable",
    }
}

/// Keep `made`, the PDF of `content`, as the preview of draft `draft`, made
/// with `layout` at `now`.
async fn store(
    db: &PgPool,
    draft: Uuid,
    content: &Content,
    made: &Made,
    (template, length): (Template, Length),
    now: DateTime<Utc>,
) -> Result<()> {
    sqlx::query(
        "UPDATE pipeline.cv_drafts
            SET preview_pdf = $2, preview_template = $3, preview_length = $4,
                preview_pages = $5, preview_checks = $6, preview_error = NULL, preview_at = $7,
                preview_of = $8
          WHERE id = $1 AND status = 'draft'",
    )
    .bind(draft)
    .bind(&made.bytes)
    .bind(template.as_str())
    .bind(i16::from(length.pages()))
    .bind(i16::try_from(made.pages).unwrap_or(i16::MAX))
    .bind(sqlx::types::Json(&made.checks))
    .bind(now)
    .bind(fingerprint(content))
    .execute(db)
    .await?;
    Ok(())
}

/// Say the preview of draft `draft` could not be made (`code`), with the
/// layout tried first.
async fn store_failure(
    db: &PgPool,
    draft: Uuid,
    code: &str,
    (template, length): (Template, Length),
    now: DateTime<Utc>,
) -> Result<()> {
    sqlx::query(
        "UPDATE pipeline.cv_drafts
            SET preview_pdf = NULL, preview_template = $3, preview_length = $4,
                preview_pages = NULL, preview_checks = NULL, preview_error = $2, preview_at = $5,
                preview_of = NULL
          WHERE id = $1 AND status = 'draft'",
    )
    .bind(draft)
    .bind(code)
    .bind(template.as_str())
    .bind(i16::from(length.pages()))
    .bind(now)
    .execute(db)
    .await?;
    Ok(())
}

/// Render the preview of draft `draft` (its `content` as it is now), trying
/// the layouts from `start` down ([`ladder`]), and keep it. Never fails the
/// change that asked for it: a render that fails is kept as a short code and
/// logged with its cause.
#[allow(clippy::too_many_arguments)]
pub(super) async fn refresh(
    db: &PgPool,
    opp: &Opp,
    draft: Uuid,
    content: &Content,
    frame: &Frame,
    language: &str,
    start: (Template, Length),
    now: DateTime<Utc>,
) {
    let mut last = None;
    for (template, length) in ladder(start) {
        match render_draft(db, opp, content, frame, language, template, length, false).await {
            Ok(Ok(made)) => {
                if let Err(e) = store(db, draft, content, &made, (template, length), now).await {
                    tracing::warn!(error = e.code(), "a CV preview could not be kept");
                }
                return;
            }
            Ok(Err(refusal @ Refusal::DoesntFit { .. })) => last = Some(refusal),
            Ok(Err(refusal)) => {
                if let Refusal::Unavailable(cause) = &refusal {
                    cause.log("cv_preview");
                }
                last = Some(refusal);
                break;
            }
            Err(e) => {
                tracing::warn!(error = e.code(), "a CV preview could not be made");
                return;
            }
        }
    }
    let Some(refusal) = last else {
        return;
    };
    if let Refusal::DoesntFit { pages, .. } = refusal {
        tracing::info!(pages, "a CV preview does not fit two pages");
    }
    if let Err(e) = store_failure(db, draft, error_code(&refusal), start, now).await {
        tracing::warn!(error = e.code(), "a CV preview's failure could not be kept");
    }
}

/// The preview's PDF of draft `row`, when it was made from `content` as it
/// is now, in `layout`, and its read-back passed: what Approve can keep as it
/// is.
pub(super) async fn reusable(
    db: &PgPool,
    row: &Row,
    content: &Content,
    layout: (Template, Length),
) -> Result<Option<Made>> {
    let fresh = row.preview_ready
        && row.preview_error.is_none()
        && layout_of(row) == Some(layout)
        && row.preview_checks.as_ref().is_some_and(|c| c.0.passed());
    let (Some(checks), Some(pages), true) = (
        row.preview_checks.as_ref(),
        row.preview_pages.and_then(|p| u32::try_from(p).ok()),
        fresh,
    ) else {
        return Ok(None);
    };
    let bytes: Option<Vec<u8>> = sqlx::query_scalar(
        "SELECT preview_pdf FROM pipeline.cv_drafts
          WHERE id = $1 AND preview_pdf IS NOT NULL AND preview_of = $2",
    )
    .bind(row.id)
    .bind(fingerprint(content))
    .fetch_optional(db)
    .await?;
    Ok(bytes.map(|bytes| Made {
        bytes,
        pages,
        checks: checks.0.clone(),
    }))
}

/// See the draft in another layout.
///
/// Renders the draft's text with the `template` and `length` asked for (what
/// the preview has when left out) and keeps it as its preview. **No model
/// call.** What the read-back finds is a note, never a refusal. Refused when
/// it does not fit the length (`cv_doesnt_fit`): the preview stays as it was.
/// Only a draft has a preview to change.
#[utoipa::path(
    post,
    path = "/api/v1/opportunities/{id}/cv/drafts/{n}/preview",
    tag = "pipeline",
    operation_id = "pipeline.cv_preview",
    security(("session" = [])),
    params(("id" = Uuid, Path, description = "The opportunity."),
        ("n" = u32, Path, description = "The draft.")),
    request_body = LayoutRequest,
    responses(
        (status = 200, description = "The CV, with the new preview.", body = Cv),
        (status = 400, description = "`invalid_layout`; `invalid_length`", body = Problem,
            content_type = "application/problem+json"),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`", body = Problem,
            content_type = "application/problem+json"),
        (status = 409, description = "`draft_frozen`; `cv_doesnt_fit`", body = Problem,
            content_type = "application/problem+json"),
        (status = 503, description = "`cv_pdf_unavailable`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn make(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(deps): Extension<PipelineDeps>,
    Path((id, n)): Path<(Uuid, u32)>,
    body: std::result::Result<Json<LayoutRequest>, JsonRejection>,
) -> Result<Json<Cv>> {
    let request = json_body(body, "invalid_layout")?;
    let account = principal.account_id;
    let now = state.clock.now();
    let (opp, row) = open_draft(&state, account, id, n, now).await?;
    let fallback = layout_of(&row).unwrap_or((Template::Clean, Length::One));
    let chosen = layout(&request, fallback)?;
    let made = render_draft(
        &state.db,
        &opp,
        &row.content.0,
        &row.frame.0,
        &row.language,
        chosen.0,
        chosen.1,
        false,
    )
    .await?;
    match made {
        Ok(made) => store(&state.db, row.id, &row.content.0, &made, chosen, now).await?,
        Err(Refusal::DoesntFit { length: 1, .. }) => {
            return Err(Error::Conflict(
                "cv_doesnt_fit",
                "Doesn't fit on 1 page: switch to 2 pages or remove a few lines".to_owned(),
            ));
        }
        Err(Refusal::DoesntFit { .. }) => {
            return Err(Error::Conflict(
                "cv_doesnt_fit",
                "Doesn't fit on 2 pages: remove a few lines".to_owned(),
            ));
        }
        Err(Refusal::Unavailable(cause)) => {
            cause.log("cv_preview");
            return Err(Error::Unavailable("cv_pdf_unavailable"));
        }
        Err(Refusal::Checks(_)) => return Err(Error::Unavailable("cv_pdf_unavailable")),
    }
    Ok(Json(build(&deps, &state.db, account, &opp, now).await?))
}

/// Whether the person is downloading, not viewing.
#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct PreviewQuery {
    /// `1` serves it as an attachment.
    pub download: Option<String>,
    /// Anything: lets the page ask for the newest preview (it is never
    /// cached).
    pub v: Option<String>,
}

#[derive(sqlx::FromRow)]
struct PreviewRow {
    pdf: Vec<u8>,
    name: Option<String>,
    company: String,
}

/// The PDF of a draft's preview.
///
/// `Content-Type: application/pdf`, shown inline, named
/// `<Name> CV - <Company> (draft).pdf`; `?download=1` serves it as an
/// attachment. Never cached, never sniffed. A draft with no preview, and
/// another account's, are not found.
#[utoipa::path(
    get,
    path = "/api/v1/opportunities/{id}/cv/drafts/{n}/preview.pdf",
    tag = "pipeline",
    operation_id = "pipeline.cv_preview_pdf",
    security(("session" = [])),
    params(("id" = Uuid, Path, description = "The opportunity."),
        ("n" = u32, Path, description = "The draft."), PreviewQuery),
    responses(
        (status = 200, description = "The PDF.", content_type = "application/pdf",
            body = PdfFile),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn pdf_file(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Path((id, n)): Path<(Uuid, u32)>,
    Query(query): Query<PreviewQuery>,
) -> Result<Response> {
    let account = principal.account_id;
    let n = i32::try_from(n).map_err(|_| Error::NotFound)?;
    let row: PreviewRow = sqlx::query_as(
        "SELECT d.preview_pdf AS pdf, d.content -> 'contact' ->> 'name' AS name, o.company
           FROM pipeline.cv_drafts d
           JOIN pipeline.opportunities o ON o.id = d.opportunity_id
          WHERE d.opportunity_id = $1 AND d.account_id = $2 AND d.n = $3
            AND d.preview_pdf IS NOT NULL
            AND (d.expires_at IS NULL OR d.expires_at > $4)",
    )
    .bind(id)
    .bind(account)
    .bind(n)
    .bind(state.clock.now())
    .fetch_optional(&state.db)
    .await?
    .ok_or(Error::NotFound)?;
    let download = matches!(query.download.as_deref(), Some("1" | "true"));
    let who = row
        .name
        .as_deref()
        .map(file_stem)
        .filter(|n| !n.is_empty())
        .map_or_else(|| "CV".to_owned(), |name| format!("{name} CV"));
    let company = file_stem(&row.company);
    let stem = if company.is_empty() {
        format!("{who} (draft)")
    } else {
        format!("{who} - {company} (draft)")
    };
    let disposition = HeaderValue::from_str(&disposition(
        if download { "attachment" } else { "inline" },
        &stem,
    ))
    .map_err(|_| Error::Unavailable("cv_pdf_unavailable"))?;
    let length = row.pdf.len();
    let mut response = Response::new(axum::body::Body::from(row.pdf));
    *response.status_mut() = StatusCode::OK;
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/pdf"),
    );
    headers.insert(header::CONTENT_DISPOSITION, disposition);
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(header::CONTENT_LENGTH, HeaderValue::from(length));
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::cv::pdf::Cause;

    #[test]
    fn the_layouts_are_tried_from_the_one_chosen_down_to_compact_on_two_pages() {
        use Length::{One, Two};
        use Template::{Clean, Compact};
        assert_eq!(
            ladder((Clean, One)),
            [(Clean, One), (Clean, Two), (Compact, Two)]
        );
        assert_eq!(ladder((Compact, One)), [(Compact, One), (Compact, Two)]);
        assert_eq!(ladder((Clean, Two)), [(Clean, Two), (Compact, Two)]);
        assert_eq!(ladder((Compact, Two)), [(Compact, Two)]);
    }

    #[test]
    fn a_failure_is_named_by_a_short_code() {
        assert_eq!(
            error_code(&Refusal::DoesntFit {
                pages: 3,
                length: 2
            }),
            "cv_doesnt_fit"
        );
        assert_eq!(
            error_code(&Refusal::Unavailable(Cause::Timeout)),
            "cv_pdf_unavailable"
        );
    }
}
