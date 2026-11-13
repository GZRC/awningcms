use crate::{
    builder,
    error::{ApiError, ApiResult},
    validation, AppState,
};
use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgConnection, Postgres, Transaction};
use uuid::Uuid;

#[derive(Serialize, FromRow)]
pub struct Site {
    id: Uuid,
    name: String,
    slug: String,
    domain: Option<String>,
    created_at: DateTime<Utc>,
    blog_enabled: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SiteInput {
    name: String,
    slug: String,
    #[serde(default)]
    domain: Option<String>,
}
impl SiteInput {
    fn validate(&mut self) -> ApiResult<()> {
        self.name = self.name.trim().to_string();
        validation::bounded(&self.name, "Le nom du site", 1, 200)?;
        validation::slug(&self.slug)?;
        self.domain = self
            .domain
            .take()
            .map(|v| v.trim().to_ascii_lowercase())
            .filter(|v| !v.is_empty());
        if let Some(domain) = &self.domain {
            validation::domain(domain)?;
        }
        Ok(())
    }
}

pub async fn sites(State(state): State<AppState>) -> ApiResult<Json<Vec<Site>>> {
    Ok(Json(sqlx::query_as("SELECT s.id,s.name,s.slug,s.domain,s.created_at,c.blog_enabled FROM cms_sites s JOIN cms_settings c ON c.site_id=s.id ORDER BY s.created_at,s.id").fetch_all(&state.pool).await?))
}
pub async fn site(
    State(state): State<AppState>,
    Path(site_id): Path<Uuid>,
) -> ApiResult<Json<Site>> {
    Ok(Json(find_site(&state, site_id).await?))
}
async fn find_site(state: &AppState, site_id: Uuid) -> ApiResult<Site> {
    sqlx::query_as("SELECT s.id,s.name,s.slug,s.domain,s.created_at,c.blog_enabled FROM cms_sites s JOIN cms_settings c ON c.site_id=s.id WHERE s.id=$1")
        .bind(site_id).fetch_optional(&state.pool).await?.ok_or_else(ApiError::missing)
}
pub async fn create_site(
    State(state): State<AppState>,
    Json(mut input): Json<SiteInput>,
) -> ApiResult<(StatusCode, Json<Site>)> {
    input.validate()?;
    let site_id = Uuid::new_v4();
    let mut tx = state.pool.begin().await?;
    sqlx::query("INSERT INTO cms_sites(id,name,slug,domain) VALUES($1,$2,$3,$4)")
        .bind(site_id)
        .bind(&input.name)
        .bind(&input.slug)
        .bind(&input.domain)
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO cms_settings(site_id,site_name) VALUES($1,$2)")
        .bind(site_id)
        .bind(&input.name)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(find_site(&state, site_id).await?)))
}
pub async fn update_site(
    State(state): State<AppState>,
    Path(site_id): Path<Uuid>,
    Json(mut input): Json<SiteInput>,
) -> ApiResult<Json<Site>> {
    input.validate()?;
    let result = sqlx::query("UPDATE cms_sites SET name=$2,slug=$3,domain=$4 WHERE id=$1")
        .bind(site_id)
        .bind(input.name)
        .bind(input.slug)
        .bind(input.domain)
        .execute(&state.pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(ApiError::missing());
    }
    Ok(Json(find_site(&state, site_id).await?))
}

#[derive(Serialize, Deserialize, FromRow)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    site_name: String,
    description: String,
    blog_enabled: bool,
}
pub async fn settings(
    State(state): State<AppState>,
    Path(site_id): Path<Uuid>,
) -> ApiResult<Json<Settings>> {
    Ok(Json(
        sqlx::query_as(
            "SELECT site_name,description,blog_enabled FROM cms_settings WHERE site_id=$1",
        )
        .bind(site_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(ApiError::missing)?,
    ))
}
pub async fn save_settings(
    State(state): State<AppState>,
    Path(site_id): Path<Uuid>,
    Json(mut input): Json<Settings>,
) -> ApiResult<Json<Settings>> {
    input.site_name = input.site_name.trim().to_string();
    validation::bounded(&input.site_name, "Le nom du site", 1, 200)?;
    validation::bounded(&input.description, "La description", 0, 500)?;
    // The same row lock used by content writes makes module toggles atomic with publication.
    let mut tx = state.pool.begin().await?;
    lock_site(&mut tx, site_id).await?;
    sqlx::query(
        "UPDATE cms_settings SET site_name=$2,description=$3,blog_enabled=$4 WHERE site_id=$1",
    )
    .bind(site_id)
    .bind(&input.site_name)
    .bind(&input.description)
    .bind(input.blog_enabled)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Json(input))
}

#[derive(Serialize, FromRow)]
pub struct Dashboard {
    pages: i64,
    articles: i64,
    drafts: i64,
    published: i64,
}
pub async fn dashboard(
    State(state): State<AppState>,
    Path(site_id): Path<Uuid>,
) -> ApiResult<Json<Dashboard>> {
    find_site(&state, site_id).await?;
    Ok(Json(sqlx::query_as("SELECT count(*) FILTER (WHERE kind='page') AS pages,count(*) FILTER (WHERE kind='article') AS articles,count(*) FILTER (WHERE published_snapshot IS NULL) AS drafts,count(*) FILTER (WHERE published_snapshot IS NOT NULL) AS published FROM cms_contents WHERE site_id=$1")
        .bind(site_id).fetch_one(&state.pool).await?))
}

#[derive(FromRow)]
struct ContentRow {
    id: Uuid,
    site_id: Uuid,
    kind: String,
    title: String,
    slug: String,
    excerpt: String,
    body: String,
    layout_draft: Option<serde_json::Value>,
    revision: i64,
    published_snapshot: Option<serde_json::Value>,
    published_slug: Option<String>,
    published_at: Option<DateTime<Utc>>,
    updated_at: DateTime<Utc>,
}
#[derive(Serialize)]
pub struct Content {
    id: Uuid,
    site_id: Uuid,
    kind: String,
    title: String,
    slug: String,
    excerpt: String,
    body: String,
    layout: Option<serde_json::Value>,
    revision: i64,
    published: bool,
    published_slug: Option<String>,
    published_at: Option<DateTime<Utc>>,
    updated_at: DateTime<Utc>,
    has_unpublished_changes: bool,
}
impl ContentRow {
    fn into_content(self) -> Content {
        let changed = self
            .published_snapshot
            .as_ref()
            .map(|snapshot| {
                snapshot.get("title").and_then(|v| v.as_str()) != Some(self.title.as_str())
                    || snapshot.get("slug").and_then(|v| v.as_str()) != Some(self.slug.as_str())
                    || snapshot.get("excerpt").and_then(|v| v.as_str())
                        != Some(self.excerpt.as_str())
                    || snapshot.get("body").and_then(|v| v.as_str()) != Some(self.body.as_str())
                    || snapshot.get("layout").filter(|value| !value.is_null())
                        != self.layout_draft.as_ref()
            })
            .unwrap_or(false);
        Content {
            id: self.id,
            site_id: self.site_id,
            kind: self.kind,
            title: self.title,
            slug: self.slug,
            excerpt: self.excerpt,
            body: self.body,
            layout: self.layout_draft,
            revision: self.revision,
            published: self.published_snapshot.is_some(),
            published_slug: self.published_slug,
            published_at: self.published_at,
            updated_at: self.updated_at,
            has_unpublished_changes: changed,
        }
    }
}
#[derive(Deserialize)]
pub struct ListQuery {
    kind: Option<String>,
}
pub async fn list(
    State(state): State<AppState>,
    Path(site_id): Path<Uuid>,
    Query(query): Query<ListQuery>,
) -> ApiResult<Json<Vec<Content>>> {
    find_site(&state, site_id).await?;
    if let Some(kind) = &query.kind {
        validate_kind(kind)?;
    }
    let rows:Vec<ContentRow>=sqlx::query_as("SELECT * FROM cms_contents WHERE site_id=$1 AND ($2::text IS NULL OR kind=$2) ORDER BY updated_at DESC,id")
        .bind(site_id).bind(query.kind).fetch_all(&state.pool).await?;
    Ok(Json(
        rows.into_iter().map(ContentRow::into_content).collect(),
    ))
}
pub async fn read(
    State(state): State<AppState>,
    Path((site_id, id)): Path<(Uuid, Uuid)>,
) -> ApiResult<Json<Content>> {
    let row: ContentRow = sqlx::query_as("SELECT * FROM cms_contents WHERE site_id=$1 AND id=$2")
        .bind(site_id)
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(ApiError::missing)?;
    Ok(Json(row.into_content()))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateInput {
    kind: String,
    title: String,
    slug: String,
    #[serde(default)]
    excerpt: String,
    #[serde(default)]
    body: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateInput {
    #[serde(default)]
    expected_revision: Option<i64>,
    title: String,
    slug: String,
    #[serde(default)]
    excerpt: String,
    #[serde(default)]
    body: String,
}
fn validate_kind(kind: &str) -> ApiResult<()> {
    if !matches!(kind, "page" | "article") {
        return Err(ApiError::invalid("Le type de contenu est invalide."));
    }
    Ok(())
}
fn validate_content(title: &str, slug: &str, excerpt: &str, body: &str) -> ApiResult<()> {
    validation::bounded(title, "Le titre", 1, 200)?;
    validation::slug(slug)?;
    validation::bounded(excerpt, "Le résumé", 0, 500)?;
    validation::bounded(body, "Le contenu", 0, 100000)?;
    Ok(())
}
// Site row locks serialize writes within one site. Every content query also scopes
// by site_id: a valid content UUID from another site must still produce a 404.
async fn lock_site(tx: &mut Transaction<'_, Postgres>, site_id: Uuid) -> ApiResult<()> {
    let exists: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM cms_sites WHERE id=$1 FOR UPDATE")
            .bind(site_id)
            .fetch_optional(&mut **tx)
            .await?;
    exists.ok_or_else(ApiError::missing)?;
    Ok(())
}
async fn ensure_article_enabled(
    connection: &mut PgConnection,
    site_id: Uuid,
    kind: &str,
) -> ApiResult<()> {
    if kind == "article" {
        let enabled: bool =
            sqlx::query_scalar("SELECT blog_enabled FROM cms_settings WHERE site_id=$1")
                .bind(site_id)
                .fetch_one(connection)
                .await?;
        if !enabled {
            return Err(ApiError::conflict("Activez le module Blog dans les modules du site pour modifier ou publier un article."));
        }
    }
    Ok(())
}
async fn check_slug(
    connection: &mut PgConnection,
    site_id: Uuid,
    id: Uuid,
    slug: &str,
) -> ApiResult<()> {
    let conflict:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM cms_contents WHERE site_id=$1 AND id<>$2 AND (slug=$3 OR published_slug=$3))").bind(site_id).bind(id).bind(slug).fetch_one(connection).await?;
    if conflict {
        return Err(ApiError::conflict(
            "Cette adresse est déjà utilisée par un contenu de ce site.",
        ));
    }
    Ok(())
}
async fn content_in_tx(
    connection: &mut PgConnection,
    site_id: Uuid,
    id: Uuid,
) -> ApiResult<ContentRow> {
    sqlx::query_as("SELECT * FROM cms_contents WHERE site_id=$1 AND id=$2")
        .bind(site_id)
        .bind(id)
        .fetch_optional(connection)
        .await?
        .ok_or_else(ApiError::missing)
}

pub async fn create(
    State(state): State<AppState>,
    Path(site_id): Path<Uuid>,
    Json(input): Json<CreateInput>,
) -> ApiResult<(StatusCode, Json<Content>)> {
    let row = create_content(
        &state,
        site_id,
        &input.kind,
        &input.title,
        &input.slug,
        &input.excerpt,
        &input.body,
    )
    .await?;
    Ok((StatusCode::CREATED, Json(row.into_content())))
}
async fn create_content(
    state: &AppState,
    site_id: Uuid,
    kind: &str,
    title: &str,
    slug: &str,
    excerpt: &str,
    body: &str,
) -> ApiResult<ContentRow> {
    let title = title.trim();
    validate_kind(kind)?;
    validate_content(title, slug, excerpt, body)?;
    let id = Uuid::new_v4();
    let mut tx = state.pool.begin().await?;
    lock_site(&mut tx, site_id).await?;
    ensure_article_enabled(&mut tx, site_id, kind).await?;
    check_slug(&mut tx, site_id, id, slug).await?;
    let row:ContentRow=sqlx::query_as("INSERT INTO cms_contents(id,site_id,kind,title,slug,excerpt,body) VALUES($1,$2,$3,$4,$5,$6,$7) RETURNING *")
        .bind(id).bind(site_id).bind(kind).bind(title).bind(slug).bind(excerpt).bind(body).fetch_one(&mut *tx).await?;
    tx.commit().await?;
    Ok(row)
}
pub async fn update(
    State(state): State<AppState>,
    Path((site_id, id)): Path<(Uuid, Uuid)>,
    Json(input): Json<UpdateInput>,
) -> ApiResult<Json<Content>> {
    let row = update_content(
        &state,
        site_id,
        id,
        input.expected_revision,
        &input.title,
        &input.slug,
        &input.excerpt,
        Some(&input.body),
    )
    .await?;
    Ok(Json(row.into_content()))
}
#[allow(clippy::too_many_arguments)]
async fn update_content(
    state: &AppState,
    site_id: Uuid,
    id: Uuid,
    expected: Option<i64>,
    title: &str,
    slug: &str,
    excerpt: &str,
    body: Option<&str>,
) -> ApiResult<ContentRow> {
    let title = title.trim();
    let mut tx = state.pool.begin().await?;
    lock_site(&mut tx, site_id).await?;
    let existing = content_in_tx(&mut tx, site_id, id).await?;
    let body = body.unwrap_or(&existing.body).to_owned();
    validate_content(title, slug, excerpt, &body)?;
    check_revision(&existing, expected)?;
    ensure_article_enabled(&mut tx, site_id, &existing.kind).await?;
    check_slug(&mut tx, site_id, id, slug).await?;
    let row:ContentRow=sqlx::query_as("UPDATE cms_contents SET title=$3,slug=$4,excerpt=$5,body=$6,revision=revision+1,updated_at=now() WHERE site_id=$1 AND id=$2 RETURNING *")
        .bind(site_id).bind(id).bind(title).bind(slug).bind(excerpt).bind(body).fetch_one(&mut *tx).await?;
    tx.commit().await?;
    Ok(row)
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct PublishRevision {
    #[serde(default)]
    expected_revision: Option<i64>,
}
fn check_revision(existing: &ContentRow, expected: Option<i64>) -> ApiResult<()> {
    if expected.is_some_and(|value| value != existing.revision) {
        return Err(ApiError::conflict(
            "Cette page a été modifiée dans une autre fenêtre. Rechargez-la avant de continuer.",
        ));
    }
    if existing.layout_draft.is_some() && expected.is_none() {
        return Err(ApiError::conflict(
            "Rechargez cette page dans le constructeur avant de la modifier ou de la publier.",
        ));
    }
    Ok(())
}
pub async fn publish(
    State(state): State<AppState>,
    Path((site_id, id)): Path<(Uuid, Uuid)>,
    body: Bytes,
) -> ApiResult<Json<Content>> {
    // Existing text clients sent an empty body. Visual documents always send
    // an expected revision, so a stale editor cannot publish someone else's draft.
    let input: PublishRevision = if body.is_empty() {
        PublishRevision::default()
    } else {
        serde_json::from_slice(&body)
            .map_err(|_| ApiError::invalid("La demande de publication est invalide."))?
    };
    let row = publish_content(&state, site_id, id, input.expected_revision).await?;
    Ok(Json(row.into_content()))
}
async fn publish_content(
    state: &AppState,
    site_id: Uuid,
    id: Uuid,
    expected: Option<i64>,
) -> ApiResult<ContentRow> {
    let mut tx = state.pool.begin().await?;
    lock_site(&mut tx, site_id).await?;
    let existing = content_in_tx(&mut tx, site_id, id).await?;
    check_revision(&existing, expected)?;
    if let Some(value) = &existing.layout_draft {
        let layout: builder::PageDocument = serde_json::from_value(value.clone())
            .map_err(|_| ApiError::invalid("Le format des sections est invalide."))?;
        layout.validate()?;
    }
    ensure_article_enabled(&mut tx, site_id, &existing.kind).await?;
    check_slug(&mut tx, site_id, id, &existing.slug).await?;
    let published_at = Utc::now();
    let snapshot = serde_json::json!({"id":id,"kind":existing.kind,"title":existing.title,"slug":existing.slug,"excerpt":existing.excerpt,"body":existing.body,"layout":existing.layout_draft,"revision":existing.revision,"published_at":published_at});
    let row:ContentRow=sqlx::query_as("UPDATE cms_contents SET published_snapshot=$3,published_slug=slug,published_at=$4,updated_at=now() WHERE site_id=$1 AND id=$2 RETURNING *")
        .bind(site_id).bind(id).bind(snapshot).bind(published_at).fetch_one(&mut *tx).await?;
    tx.commit().await?;
    Ok(row)
}
pub async fn unpublish(
    State(state): State<AppState>,
    Path((site_id, id)): Path<(Uuid, Uuid)>,
    body: Bytes,
) -> ApiResult<Json<Content>> {
    let input: PublishRevision = if body.is_empty() {
        PublishRevision::default()
    } else {
        serde_json::from_slice(&body)
            .map_err(|_| ApiError::invalid("La demande de retrait est invalide."))?
    };
    let mut tx = state.pool.begin().await?;
    lock_site(&mut tx, site_id).await?;
    let existing = content_in_tx(&mut tx, site_id, id).await?;
    check_revision(&existing, input.expected_revision)?;
    let row:ContentRow=sqlx::query_as("UPDATE cms_contents SET published_snapshot=NULL,published_slug=NULL,published_at=NULL,revision=revision+1,updated_at=now() WHERE site_id=$1 AND id=$2 RETURNING *")
        .bind(site_id).bind(id).fetch_optional(&mut *tx).await?.ok_or_else(ApiError::missing)?;
    tx.commit().await?;
    Ok(Json(row.into_content()))
}
pub async fn delete(
    State(state): State<AppState>,
    Path((site_id, id)): Path<(Uuid, Uuid)>,
) -> ApiResult<StatusCode> {
    let mut tx = state.pool.begin().await?;
    lock_site(&mut tx, site_id).await?;
    let result = sqlx::query("DELETE FROM cms_contents WHERE site_id=$1 AND id=$2")
        .bind(site_id)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    if result.rows_affected() == 0 {
        return Err(ApiError::missing());
    }
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize, FromRow)]
pub struct PublicSite {
    id: Uuid,
    name: String,
    slug: String,
    domain: Option<String>,
    site_name: String,
    description: String,
    blog_enabled: bool,
}
async fn find_public_site(state: &AppState, slug: Option<&str>) -> ApiResult<PublicSite> {
    sqlx::query_as("SELECT s.id,s.name,s.slug,s.domain,c.site_name,c.description,c.blog_enabled FROM cms_sites s JOIN cms_settings c ON c.site_id=s.id WHERE (($1::text IS NULL AND s.is_primary) OR s.slug=$1)")
        .bind(slug).fetch_optional(&state.pool).await?.ok_or_else(ApiError::missing)
}
pub async fn primary_site(State(state): State<AppState>) -> ApiResult<Json<PublicSite>> {
    Ok(Json(find_public_site(&state, None).await?))
}
pub async fn public_site(
    State(state): State<AppState>,
    Path(site_slug): Path<String>,
) -> ApiResult<Json<PublicSite>> {
    Ok(Json(find_public_site(&state, Some(&site_slug)).await?))
}
async fn published_list(
    state: &AppState,
    site: PublicSite,
) -> ApiResult<Json<Vec<serde_json::Value>>> {
    let snapshots:Vec<serde_json::Value>=sqlx::query_scalar("SELECT c.published_snapshot - 'body' FROM cms_contents c JOIN cms_settings s ON s.site_id=c.site_id WHERE c.site_id=$1 AND c.published_snapshot IS NOT NULL AND (c.kind='page' OR s.blog_enabled) ORDER BY c.published_at DESC,c.id")
        .bind(site.id).fetch_all(&state.pool).await?;
    Ok(Json(snapshots))
}
async fn published_read(
    state: &AppState,
    site: PublicSite,
    slug: &str,
) -> ApiResult<Json<serde_json::Value>> {
    let snapshot:serde_json::Value=sqlx::query_scalar("SELECT c.published_snapshot FROM cms_contents c JOIN cms_settings s ON s.site_id=c.site_id WHERE c.site_id=$1 AND c.published_slug=$2 AND c.published_snapshot IS NOT NULL AND (c.kind='page' OR s.blog_enabled)")
        .bind(site.id).bind(slug).fetch_optional(&state.pool).await?.ok_or_else(ApiError::missing)?;
    Ok(Json(snapshot))
}
pub async fn public_list(State(state): State<AppState>) -> ApiResult<Json<Vec<serde_json::Value>>> {
    published_list(&state, find_public_site(&state, None).await?).await
}
pub async fn public_read(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    published_read(&state, find_public_site(&state, None).await?, &slug).await
}
pub async fn site_public_list(
    State(state): State<AppState>,
    Path(site_slug): Path<String>,
) -> ApiResult<Json<Vec<serde_json::Value>>> {
    published_list(&state, find_public_site(&state, Some(&site_slug)).await?).await
}
pub async fn site_public_read(
    State(state): State<AppState>,
    Path((site_slug, slug)): Path<(String, String)>,
) -> ApiResult<Json<serde_json::Value>> {
    published_read(
        &state,
        find_public_site(&state, Some(&site_slug)).await?,
        &slug,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn published_snapshot_survives_draft_edit_and_reports_changes() {
        let row = ContentRow {
            id: Uuid::new_v4(),
            site_id: Uuid::new_v4(),
            kind: "page".into(),
            title: "New title".into(),
            slug: "new-url".into(),
            excerpt: "".into(),
            body: "New text".into(),
            layout_draft: None,
            revision: 2,
            published_snapshot: Some(
                serde_json::json!({"title":"Original title","slug":"old-url","excerpt":"","body":"Original text"}),
            ),
            published_slug: Some("old-url".into()),
            published_at: Some(Utc::now()),
            updated_at: Utc::now(),
        };
        let content = row.into_content();
        assert!(content.published);
        assert!(content.has_unpublished_changes);
        assert_eq!(content.published_slug.as_deref(), Some("old-url"));
        assert_eq!(content.slug, "new-url");
    }
}
