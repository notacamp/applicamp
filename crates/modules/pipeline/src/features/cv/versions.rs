//! The tailored CV as PDF versions (#38).
//!
//! Approving a draft renders it (Clean on one page unless the person chose
//! otherwise), reads the PDF back, and keeps the PDF as **version n** in the
//! same transaction that approves: if the PDF cannot be made, or the read-back
//! finds it lacking, nothing is approved and the reasons are shown.
//!
//! A new template or length on an approved CV is a **new version** of the
//! same approved text, with no AI call. Versions are never overwritten: they
//! are what was sent. A different language is a new draft.

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
    Cv, Opp, Row, build, counted, json_body, load,
    model::{Content, Frame},
    pdf::{self, Length, Made, Refusal, Template},
    view::{DraftStatus, KeywordState, keywords},
    wants_of,
};
use crate::{PipelineDeps, features::brief::BriefWant};

/// PDF versions one opportunity can have.
pub const CAP_VERSIONS: i64 = 20;
/// Most rows `GET /api/v1/cvs` lists.
pub const MAX_LISTED: i64 = 200;

/// The look of a PDF.
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct LayoutRequest {
    /// `clean` (the default) or `compact`.
    #[serde(default)]
    pub template: Option<Template>,
    /// The most pages: 1 (the default) or 2.
    #[serde(default)]
    pub length: Option<u8>,
}

/// The template and length a request asks for, `fallback` for what it leaves out.
pub(super) fn layout(
    request: &LayoutRequest,
    fallback: (Template, Length),
) -> Result<(Template, Length)> {
    let length = match request.length {
        Some(pages) => Length::from_pages(pages).ok_or_else(|| {
            Error::BadRequest("invalid_length", "Choose 1 or 2 pages.".to_owned())
        })?,
        None => fallback.1,
    };
    Ok((request.template.unwrap_or(fallback.0), length))
}

/// The job keywords the review shows as on the CV: what the PDF must keep.
fn keywords_on_cv(wants: &[BriefWant], opp: &Opp, content: &Content, frame: &Frame) -> Vec<String> {
    keywords(
        wants,
        &opp.title,
        opp.description.as_deref().unwrap_or_default(),
        content,
        frame,
    )
    .items
    .into_iter()
    .filter(|k| k.state == KeywordState::OnCv)
    .map(|k| k.label)
    .collect()
}

/// The PDF of `content`, or why it cannot be made.
pub(super) async fn render(
    db: &PgPool,
    opp: &Opp,
    content: &Content,
    frame: &Frame,
    language: &str,
    template: Template,
    length: Length,
) -> Result<Made> {
    let wants = wants_of(db, opp.id).await?;
    let keywords = keywords_on_cv(&wants, opp, content, frame);
    pdf::make(content, frame, language, template, length, keywords)
        .await
        .map_err(|refusal| match refusal {
            Refusal::DoesntFit { length: 1, .. } => Error::Conflict(
                "cv_doesnt_fit",
                "Doesn't fit on 1 page: switch to 2 pages or remove a few lines".to_owned(),
            ),
            Refusal::DoesntFit { .. } => Error::Conflict(
                "cv_doesnt_fit",
                "Doesn't fit on 2 pages: remove a few lines".to_owned(),
            ),
            Refusal::Checks(checks) => {
                // Counts only: the lines are the person's.
                tracing::warn!(
                    headings = checks.sections.missing.len(),
                    keywords = checks.keywords.missing.len(),
                    lines = checks.lines.missing.len(),
                    "a CV PDF failed its read-back"
                );
                Error::Conflict("cv_pdf_check_failed", checks.reasons().join("\n"))
            }
            Refusal::Unavailable => Error::Unavailable("cv_pdf_unavailable"),
        })
}

/// The next version number of `opp` (the numbers drafts were approved with
/// before PDFs count too), with the opportunity's versions locked against a
/// second request choosing the same.
pub(super) async fn next_version(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    opp: Uuid,
) -> Result<i32> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(format!("cv_versions:{opp}"))
        .execute(&mut **tx)
        .await?;
    Ok(sqlx::query_scalar(
        "SELECT greatest(
            coalesce((SELECT max(version) FROM pipeline.cv_versions WHERE opportunity_id = $1), 0),
            coalesce((SELECT max(version) FROM pipeline.cv_drafts
                       WHERE opportunity_id = $1 AND status = 'approved'), 0)
         ) + 1",
    )
    .bind(opp)
    .fetch_one(&mut **tx)
    .await?)
}

/// Keep `made` as version `version` of `draft`.
#[allow(clippy::too_many_arguments)]
pub(super) async fn insert(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    account: Uuid,
    opp: Uuid,
    draft: Uuid,
    version: i32,
    language: &str,
    template: Template,
    length: Length,
    made: &Made,
    now: DateTime<Utc>,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO pipeline.cv_versions
            (id, account_id, opportunity_id, draft_id, version, template, length, language,
             pdf, size_bytes, pages, checks, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
    )
    .bind(Uuid::now_v7())
    .bind(account)
    .bind(opp)
    .bind(draft)
    .bind(version)
    .bind(template.as_str())
    .bind(i16::from(length.pages()))
    .bind(language)
    .bind(&made.bytes)
    .bind(i32::try_from(made.bytes.len()).map_err(|_| Error::Unavailable("cv_pdf_unavailable"))?)
    .bind(i16::try_from(made.pages).map_err(|_| Error::Unavailable("cv_pdf_unavailable"))?)
    .bind(sqlx::types::Json(&made.checks))
    .bind(now)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

#[derive(sqlx::FromRow)]
struct Latest {
    template: String,
    length: i16,
}

/// Make another version of the approved CV.
///
/// The latest approved draft's text, rendered with `template` and `length`
/// (what the newest version has when left out), read back, and kept as the
/// next version. **No model call.** The earlier versions are untouched. Refused
/// when the text does not fit the length (`cv_doesnt_fit`) or the read-back
/// finds the PDF lacking (`cv_pdf_check_failed`, with the reasons, one per
/// line). At most 20 versions per opportunity.
#[utoipa::path(
    post,
    path = "/api/v1/opportunities/{id}/cv/versions",
    tag = "pipeline",
    operation_id = "pipeline.cv_version",
    security(("session" = [])),
    params(("id" = Uuid, Path, description = "The opportunity.")),
    request_body = LayoutRequest,
    responses(
        (status = 200, description = "The CV, with the new version.", body = Cv),
        (status = 400, description = "`invalid_layout`; `invalid_length`", body = Problem,
            content_type = "application/problem+json"),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`", body = Problem,
            content_type = "application/problem+json"),
        (status = 409, description = "`not_approved`; `same_layout`; `cv_doesnt_fit`; \
            `cv_pdf_check_failed`", body = Problem, content_type = "application/problem+json"),
        (status = 429, description = "`cv_versions_cap`", body = Problem,
            content_type = "application/problem+json"),
        (status = 503, description = "`cv_pdf_unavailable`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn create(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(deps): Extension<PipelineDeps>,
    Path(id): Path<Uuid>,
    body: std::result::Result<Json<LayoutRequest>, JsonRejection>,
) -> Result<Json<Cv>> {
    let request = json_body(body, "invalid_layout")?;
    let account = principal.account_id;
    let now = state.clock.now();
    let db = &state.db;
    let opp = load(db, account, id).await?;
    let row: Row = sqlx::query_as(
        "SELECT id, n, language, status, version, content, frame, checks, created_at,
                approved_at, expires_at
           FROM pipeline.cv_drafts
          WHERE opportunity_id = $1 AND account_id = $2 AND status = 'approved'
          ORDER BY n DESC LIMIT 1",
    )
    .bind(opp.id)
    .bind(account)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| {
        Error::Conflict(
            "not_approved",
            "Approve the CV first: a PDF is made from the approved text.".to_owned(),
        )
    })?;
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM pipeline.cv_versions WHERE opportunity_id = $1")
            .bind(opp.id)
            .fetch_one(db)
            .await?;
    if count >= CAP_VERSIONS {
        return Err(Error::TooManyRequests(
            "cv_versions_cap",
            format!("You can keep {CAP_VERSIONS} PDF versions of one CV."),
        ));
    }
    let newest: Option<Latest> = sqlx::query_as(
        "SELECT template, length FROM pipeline.cv_versions
          WHERE opportunity_id = $1 ORDER BY version DESC LIMIT 1",
    )
    .bind(opp.id)
    .fetch_optional(db)
    .await?;
    let fallback = newest
        .as_ref()
        .and_then(|v| {
            Some((
                Template::parse(&v.template)?,
                Length::from_pages(u8::try_from(v.length).ok()?)?,
            ))
        })
        .unwrap_or((Template::Clean, Length::One));
    let (template, length) = layout(&request, fallback)?;
    if let Some(newest) = &newest
        && newest.template == template.as_str()
        && i16::from(length.pages()) == newest.length
    {
        return Err(Error::Conflict(
            "same_layout",
            "That is the layout of the newest version already.".to_owned(),
        ));
    }
    let made = render(
        db,
        &opp,
        &row.content.0,
        &row.frame.0,
        &row.language,
        template,
        length,
    )
    .await?;
    let mut tx = db.begin().await?;
    let version = next_version(&mut tx, opp.id).await?;
    insert(
        &mut tx,
        account,
        opp.id,
        row.id,
        version,
        &row.language,
        template,
        length,
        &made,
        now,
    )
    .await?;
    tx.commit().await?;
    tracing::info!(version, pages = made.pages, "a CV PDF version made");
    Ok(Json(build(&deps, db, account, &opp, now).await?))
}

/// `name` as a file name: no path or quoting characters, one line.
fn file_stem(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_control()
                || matches!(
                    c,
                    '/' | '\\' | '"' | ';' | ':' | '*' | '?' | '<' | '>' | '|'
                )
            {
                ' '
            } else {
                c
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(120)
        .collect()
}

/// `text` in plain ASCII for the old `filename=` parameter: accents folded,
/// anything else `_`.
fn ascii_fold(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            c if c.is_ascii() => c,
            'à'..='å' | 'ā' | 'ă' | 'ą' => 'a',
            'À'..='Å' | 'Ā' | 'Ă' | 'Ą' => 'A',
            'ç' | 'ć' | 'č' => 'c',
            'Ç' | 'Ć' | 'Č' => 'C',
            'è'..='ë' | 'ē' | 'ė' | 'ę' | 'ě' => 'e',
            'È'..='Ë' | 'Ē' | 'Ė' | 'Ę' | 'Ě' => 'E',
            'ì'..='ï' | 'ī' | 'į' => 'i',
            'Ì'..='Ï' | 'Ī' | 'Į' => 'I',
            'ñ' | 'ń' | 'ň' => 'n',
            'Ñ' | 'Ń' | 'Ň' => 'N',
            'ò'..='ö' | 'ø' | 'ō' | 'ő' => 'o',
            'Ò'..='Ö' | 'Ø' | 'Ō' | 'Ő' => 'O',
            'ù'..='ü' | 'ū' | 'ů' | 'ű' => 'u',
            'Ù'..='Ü' | 'Ū' | 'Ů' | 'Ű' => 'U',
            'ý' | 'ÿ' => 'y',
            'Ý' | 'Ÿ' => 'Y',
            'š' | 'ś' => 's',
            'Š' | 'Ś' => 'S',
            'ž' | 'ź' | 'ż' => 'z',
            'Ž' | 'Ź' | 'Ż' => 'Z',
            'ł' => 'l',
            'Ł' => 'L',
            'ß' => 's',
            _ => '_',
        })
        .collect()
}

/// RFC 5987 percent-encoding of `text`'s UTF-8 bytes.
fn percent_encode(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric()
            || matches!(
                byte,
                b'!' | b'#' | b'$' | b'&' | b'+' | b'-' | b'.' | b'^' | b'_' | b'`' | b'|' | b'~'
            )
        {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// The `Content-Disposition` of a PDF named `stem`: the ASCII fallback and
/// the real name.
fn disposition(kind: &str, stem: &str) -> String {
    let file = format!("{stem}.pdf");
    let fallback = ascii_fold(&file).replace('"', "_");
    format!(
        "{kind}; filename=\"{fallback}\"; filename*=UTF-8''{}",
        percent_encode(&file)
    )
}

/// Whether the person is downloading, not viewing.
#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct PdfQuery {
    /// `1` serves it as an attachment and records the download.
    pub download: Option<String>,
}

/// The bytes of a PDF (documentation only: the answer is the file itself).
#[derive(ToSchema)]
#[schema(value_type = String, format = Binary)]
#[allow(dead_code)]
pub struct PdfFile(Vec<u8>);

#[derive(sqlx::FromRow)]
struct PdfRow {
    pdf: Vec<u8>,
    name: Option<String>,
    company: String,
}

/// The PDF of a version.
///
/// `Content-Type: application/pdf`, shown inline, named
/// `<Name> CV - <Company>.pdf`. `?download=1` serves it as an attachment and
/// sets the version's `downloaded_at`. Never cached, never sniffed. Another
/// account's version is not found.
#[utoipa::path(
    get,
    path = "/api/v1/opportunities/{id}/cv/versions/{version}/pdf",
    tag = "pipeline",
    operation_id = "pipeline.cv_pdf",
    security(("session" = [])),
    params(("id" = Uuid, Path, description = "The opportunity."),
        ("version" = u32, Path, description = "The version."), PdfQuery),
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
    Path((id, version)): Path<(Uuid, u32)>,
    Query(query): Query<PdfQuery>,
) -> Result<Response> {
    let account = principal.account_id;
    let version = i32::try_from(version).map_err(|_| Error::NotFound)?;
    let row: PdfRow = sqlx::query_as(
        "SELECT v.pdf, d.content -> 'contact' ->> 'name' AS name, o.company
           FROM pipeline.cv_versions v
           JOIN pipeline.cv_drafts d ON d.id = v.draft_id
           JOIN pipeline.opportunities o ON o.id = v.opportunity_id
          WHERE v.opportunity_id = $1 AND v.account_id = $2 AND v.version = $3",
    )
    .bind(id)
    .bind(account)
    .bind(version)
    .fetch_optional(&state.db)
    .await?
    .ok_or(Error::NotFound)?;
    let download = matches!(query.download.as_deref(), Some("1" | "true"));
    if download {
        sqlx::query(
            "UPDATE pipeline.cv_versions SET downloaded_at = $4
              WHERE opportunity_id = $1 AND account_id = $2 AND version = $3",
        )
        .bind(id)
        .bind(account)
        .bind(version)
        .bind(state.clock.now())
        .execute(&state.db)
        .await?;
    }
    let who = row
        .name
        .as_deref()
        .map(file_stem)
        .filter(|n| !n.is_empty())
        .map_or_else(|| "CV".to_owned(), |name| format!("{name} CV"));
    let company = file_stem(&row.company);
    let stem = if company.is_empty() {
        who
    } else {
        format!("{who} - {company}")
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

// ---- every CV ---------------------------------------------------------------

/// What a row of the CVs page is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CvKind {
    /// A PDF that was made: view it, download it.
    Version,
    /// A draft being reviewed (or an approved one with no PDF yet): review it.
    Draft,
}

/// One row of the CVs page.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CvListItem {
    pub kind: CvKind,
    pub opportunity_id: Uuid,
    /// The role.
    pub title: String,
    pub company: String,
    /// The version, for a PDF.
    pub version: Option<u32>,
    /// The draft it comes from.
    pub draft: u32,
    /// `approved` for a version, `draft` for a draft being reviewed.
    pub status: DraftStatus,
    pub template: Option<Template>,
    pub pages: Option<u32>,
    pub size_bytes: Option<u64>,
    pub language: String,
    pub created_at: DateTime<Utc>,
    pub downloaded_at: Option<DateTime<Utc>>,
    /// The newest version of its opportunity.
    pub current: bool,
}

/// Every CV of the account.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CvList {
    /// Newest first: every PDF version and every draft being reviewed, across
    /// the opportunities (at most 200).
    pub items: Vec<CvListItem>,
}

#[derive(sqlx::FromRow)]
struct ListRow {
    kind: String,
    opportunity_id: Uuid,
    title: String,
    company: String,
    version: Option<i32>,
    draft: i32,
    status: String,
    template: Option<String>,
    pages: Option<i16>,
    size_bytes: Option<i32>,
    language: String,
    created_at: DateTime<Utc>,
    downloaded_at: Option<DateTime<Utc>>,
    current: bool,
}

/// Every CV the person has.
///
/// The PDF versions and the drafts still being reviewed, across all the
/// opportunities of the account (the ones put away included: a PDF is what was
/// sent), newest first. Never calls a model.
#[utoipa::path(
    get,
    path = "/api/v1/cvs",
    tag = "pipeline",
    operation_id = "pipeline.cvs",
    security(("session" = [])),
    responses(
        (status = 200, description = "The CVs.", body = CvList),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn list_all(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
) -> Result<Json<CvList>> {
    let account = principal.account_id;
    let now = state.clock.now();
    let rows: Vec<ListRow> = sqlx::query_as(
        "SELECT * FROM (
            SELECT 'version' AS kind, o.id AS opportunity_id, o.title, o.company,
                   v.version, d.n AS draft, 'approved' AS status, v.template,
                   v.pages, v.size_bytes, v.language, v.created_at, v.downloaded_at,
                   v.version = (SELECT max(x.version) FROM pipeline.cv_versions x
                                 WHERE x.opportunity_id = v.opportunity_id) AS current
              FROM pipeline.cv_versions v
              JOIN pipeline.opportunities o ON o.id = v.opportunity_id
              JOIN pipeline.cv_drafts d ON d.id = v.draft_id
             WHERE v.account_id = $1
            UNION ALL
            SELECT 'draft', o.id, o.title, o.company, d.version, d.n, d.status,
                   NULL::text, NULL::smallint, NULL::integer, d.language, d.created_at,
                   NULL::timestamptz, false
              FROM pipeline.cv_drafts d
              JOIN pipeline.opportunities o ON o.id = d.opportunity_id
             WHERE d.account_id = $1
               AND ((d.status = 'draft' AND d.expires_at > $2)
                    OR (d.status = 'approved' AND NOT EXISTS (
                        SELECT 1 FROM pipeline.cv_versions x WHERE x.draft_id = d.id)))
         ) all_cvs
         ORDER BY created_at DESC, opportunity_id, version DESC NULLS LAST
         LIMIT $3",
    )
    .bind(account)
    .bind(now)
    .bind(MAX_LISTED)
    .fetch_all(&state.db)
    .await?;
    let items = rows
        .into_iter()
        .map(|row| CvListItem {
            kind: if row.kind == "version" {
                CvKind::Version
            } else {
                CvKind::Draft
            },
            opportunity_id: row.opportunity_id,
            title: row.title,
            company: row.company,
            version: row.version.map(|v| counted(i64::from(v))),
            draft: counted(i64::from(row.draft)),
            status: DraftStatus::parse(&row.status),
            template: row.template.as_deref().and_then(Template::parse),
            pages: row.pages.map(|p| counted(i64::from(p))),
            size_bytes: row.size_bytes.map(|b| u64::try_from(b).unwrap_or(0)),
            language: row.language,
            created_at: row.created_at,
            downloaded_at: row.downloaded_at,
            current: row.current,
        })
        .collect();
    Ok(Json(CvList { items }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_file_name_is_the_name_and_the_company_with_an_ascii_fallback_and_the_real_name() {
        let header = disposition("inline", "Joana Gonçalves CV - Example Co");
        assert!(header.starts_with("inline; filename=\"Joana Goncalves CV - Example Co.pdf\";"));
        assert!(
            header.ends_with("filename*=UTF-8''Joana%20Gon%C3%A7alves%20CV%20-%20Example%20Co.pdf")
        );
        assert_eq!(file_stem("A/B \"C\";\nD"), "A B C D");
        assert!(!disposition("attachment", &file_stem("x\"y")).contains("\"y"));
    }

    #[test]
    fn a_layout_asks_for_one_or_two_pages_and_keeps_what_it_leaves_out() {
        let keep = (Template::Compact, Length::Two);
        let none = LayoutRequest::default();
        assert_eq!(layout(&none, keep).map(|l| (l.0, l.1)).ok(), Some(keep));
        let three = LayoutRequest {
            template: None,
            length: Some(3),
        };
        assert!(layout(&three, keep).is_err());
        let one = LayoutRequest {
            template: Some(Template::Clean),
            length: Some(1),
        };
        assert_eq!(
            layout(&one, keep).map(|l| (l.0, l.1)).ok(),
            Some((Template::Clean, Length::One))
        );
    }
}
