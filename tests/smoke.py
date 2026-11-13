#!/usr/bin/env python3
"""Exercise the CMS against an EMPTY, DISPOSABLE PostgreSQL database.

The caller must set CMS_TEST_ALLOW_SETUP=1, CMS_TEST_BASE_URL (loopback only),
CMS_TEST_ORIGIN (the backend's PUBLIC_ORIGIN), and CMS_TEST_SETUP_TOKEN.
This script creates a test owner and sites, changes settings, and creates/deletes
content. The caller must discard the test database afterwards.
It deliberately refuses an already initialized CMS and never follows redirects.

Production session cookies remain Secure. This loopback-only test client sends
the cookie explicitly over local HTTP so the production cookie policy does not
need a weaker test configuration. Tokens, passwords, and response bodies are
never printed.
"""

from __future__ import annotations

import http.cookies
import html
from html.parser import HTMLParser
import ipaddress
import json
import os
import secrets
import subprocess
import uuid
import sys
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import dataclass


class TestFailure(Exception):
    """A deliberately non-sensitive failure message."""


def require(condition: bool, message: str) -> None:
    if not condition:
        raise TestFailure(message)


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


@dataclass
class Response:
    status: int
    payload: object
    headers: object


DEFAULT_ORIGIN = object()


class Client:
    def __init__(self, base: str, origin: str, cookie_name: str = "__Host-cms_session"):
        self.base = base
        self.origin = origin
        self.cookie = ""
        self.cookie_name = cookie_name
        # Proxy environment variables must never send test credentials outside
        # this explicitly local test service.
        self.opener = urllib.request.build_opener(
            urllib.request.ProxyHandler({}), NoRedirect()
        )

    def request(
        self,
        method: str,
        path: str,
        payload: object = None,
        *,
        status: int = 200,
        origin: object = DEFAULT_ORIGIN,
        content_type: str = "application/json",
        response_format: str = "json",
        extra_headers: dict | None = None,
    ) -> Response:
        require(path.startswith("/") and not path.startswith("//"), "Invalid test path")
        require("#" not in path, "Test path must not contain a fragment")
        headers = {"Accept": "text/html" if response_format == "html" else "application/json"}
        if extra_headers:
            headers.update(extra_headers)
        if self.cookie:
            headers["Cookie"] = self.cookie
        data = None
        if method != "GET":
            data = json.dumps({} if payload is None else payload).encode("utf-8")
            headers["Content-Type"] = content_type
            selected_origin = self.origin if origin is DEFAULT_ORIGIN else origin
            if selected_origin is not None:
                headers["Origin"] = str(selected_origin)
        request = urllib.request.Request(
            self.base + path, data=data, headers=headers, method=method
        )
        try:
            response = self.opener.open(request, timeout=15)
        except urllib.error.HTTPError as error:
            response = error
        except (urllib.error.URLError, TimeoutError, OSError) as error:
            # Exceptions can contain request details. Only expose the class.
            raise TestFailure(
                f"{method} {path}: connection failed ({type(error).__name__})"
            ) from None
        with response:
            actual_status = response.code
            response_headers = response.headers
            body = response.read(8 * 1024 * 1024 + 1)
        require(
            actual_status == status,
            f"{method} {path}: expected HTTP {status}, got {actual_status}",
        )
        require(len(body) <= 8 * 1024 * 1024, "Unexpectedly large response")
        if response_format == "none":
            parsed = None
        elif response_format in ("html", "markdown"):
            expected_type = "text/html" if response_format == "html" else "text/markdown"
            require(response_headers.get_content_type() == expected_type, f"{method} {path}: response is not {expected_type}")
            try:
                parsed = body.decode("utf-8")
            except UnicodeDecodeError:
                raise TestFailure(f"{method} {path}: HTML is not UTF-8") from None
        elif body:
            try:
                parsed = json.loads(body)
            except (ValueError, UnicodeDecodeError):
                raise TestFailure(f"{method} {path}: response is not valid JSON") from None
        else:
            parsed = None
        for header in response_headers.get_all("Set-Cookie", []):
            parsed_cookies = http.cookies.SimpleCookie()
            try:
                parsed_cookies.load(header)
            except http.cookies.CookieError:
                raise TestFailure("Invalid Set-Cookie header") from None
            for name, morsel in parsed_cookies.items():
                if name == self.cookie_name:
                    self.cookie = (
                        "" if morsel["max-age"] == "0" or not morsel.value
                        else name + "=" + morsel.value
                    )
        return Response(actual_status, parsed, response_headers)

    def request_html(self, path: str, *, status: int = 200) -> str:
        response = self.request("GET", path, status=status, response_format="html")
        require(isinstance(response.payload, str), "Expected an HTML document")
        return response.payload


class DocumentMetadata(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.title = ""
        self.description = None
        self.canonical = None
        self.in_title = False

    def handle_starttag(self, tag, attrs):
        attributes = dict(attrs)
        if tag == "title":
            self.in_title = True
        elif tag == "meta" and attributes.get("name") == "description":
            self.description = attributes.get("content")
        elif tag == "link" and attributes.get("rel") == "canonical":
            self.canonical = attributes.get("href")

    def handle_endtag(self, tag):
        if tag == "title":
            self.in_title = False

    def handle_data(self, data):
        if self.in_title:
            self.title += data


def assert_public_metadata(document: str, title: str, description: str, canonical: str) -> None:
    metadata = DocumentMetadata()
    metadata.feed(document)
    require(title in metadata.title, "Published title is missing from server-rendered metadata")
    require(metadata.description == description, "Server-rendered description differs from the published excerpt")
    require(metadata.canonical == canonical, "Canonical URL differs from the public content URL")


def assert_session_cookie(response: Response, cookie_name: str = "__Host-cms_session") -> None:
    found = False
    for header in response.headers.get_all("Set-Cookie", []):
        cookies = http.cookies.SimpleCookie()
        cookies.load(header)
        for name, morsel in cookies.items():
            if name != cookie_name:
                continue
            found = True
            require(bool(morsel.value), "Session cookie is empty")
            require(bool(morsel["secure"]), "Session cookie must be Secure")
            require(bool(morsel["httponly"]), "Session cookie must be HttpOnly")
            require(morsel["samesite"].lower() == "strict", "Session must use SameSite=Strict")
            require(morsel["path"] == "/", "Session cookie must have Path=/")
            require(not morsel["domain"], "__Host- session cookie must not set Domain")
    require(found, "Response did not issue the expected session cookie")


def as_object(response: Response) -> dict:
    require(isinstance(response.payload, dict), "Expected a JSON object")
    return response.payload


def as_array(response: Response) -> list:
    require(isinstance(response.payload, list), "Expected a JSON array")
    return response.payload


def extranet_checks(owner: Client, client_base: str, primary: dict, foreign_sites: list[dict], nonce: str, ok) -> None:
    """Client credentials and every permitted mutation stay scoped to Host's site."""
    cookie_name = "__Host-extranet_session"
    tenant = Client(client_base, client_base, cookie_name)
    visitor = Client(client_base, client_base, cookie_name)
    primary_id = primary["id"]
    central_url = "/api/sites/" + primary_id + "/extranet-access"
    client_url = "/extranet/api/sites/" + primary_id
    email = f"client-{nonce}@example.test"
    password = secrets.token_urlsafe(32)
    credentials = {"email": email, "password": password}

    def fragment_token(url: object, key: str) -> str:
        require(isinstance(url, str), "Extranet access URL is missing")
        parsed = urllib.parse.urlsplit(url)
        require(parsed.scheme + "://" + parsed.netloc == client_base and parsed.path == "/extranet", "Extranet access link changed the client origin")
        values = urllib.parse.parse_qs(parsed.fragment).get(key, [])
        require(len(values) == 1 and bool(values[0]), "Extranet link is missing its single-use fragment token")
        require(not parsed.query, "Extranet credentials must not appear in the URL query")
        return values[0]

    for path in ["/extranet", "/extranet/", "/extranet/pages", "/extranet?site=" + foreign_sites[0]["id"]]:
        shell = visitor.request("GET", path, response_format="html")
        require(shell.headers.get("X-CMS-Extranet") == primary_id, "Client extranet shell resolved the wrong site")
        require(shell.headers.get("Location") is None, "Client extranet must not redirect to the central CMS")
        require(shell.headers.get("Cache-Control") == "no-store", "Client extranet shell must not be cached")
    visitor.request("GET", "/extranet/api/auth/me", status=401)
    bootstrap = as_object(visitor.request("GET", "/extranet/api/bootstrap"))
    require(isinstance(bootstrap.get("site"), dict) and bootstrap["site"].get("id") == primary_id, "Extranet bootstrap differs from the Host's site")
    visitor.request_html("/s/" + foreign_sites[0]["slug"], status=404)
    visitor.request("GET", "/s/" + foreign_sites[0]["slug"] + "/extranet", status=404)
    visitor.request("GET", "/", status=404, response_format="html", extra_headers={
        "Host": "unknown.example.test", "X-Forwarded-Host": urllib.parse.urlsplit(owner.origin).netloc,
    })
    visitor.request("GET", "/extranet", status=404, extra_headers={"Host": "unknown.example.test"})
    ok("extranet stays on the client origin and selects its site from Host")

    before = as_object(owner.request("GET", central_url))
    require(before.get("configured") is False, "Disposable site unexpectedly has an extranet account")
    invitation = as_object(owner.request("POST", central_url, {}))
    token = fragment_token(invitation.get("activation_url"), "activation")
    activation = {"token": token, "name": "Client account", **credentials}
    visitor.request("POST", "/extranet/api/activate", activation, origin=owner.origin, status=403)
    visitor.request("POST", "/extranet/api/activate", {**activation, "token": secrets.token_urlsafe(32)}, status=403)
    activated = tenant.request("POST", "/extranet/api/activate", activation, status=201)
    assert_session_cookie(activated, cookie_name)
    require(as_object(activated).get("email") == email, "Extranet activation created another identity")
    visitor.request("POST", "/extranet/api/activate", activation, status=403)
    configured = as_object(owner.request("GET", central_url))
    require(configured.get("configured") is True, "Activated extranet account is not visible to its owner")
    require(as_object(tenant.request("GET", "/extranet/api/auth/me")).get("email") == email, "Client session does not identify the client account")
    ok("client activation requires its own origin and a single-use invitation")

    central_cookie_client = Client(client_base, client_base, cookie_name)
    central_cookie_client.cookie = owner.cookie
    central_cookie_client.request("GET", "/extranet/api/auth/me", status=401)
    client_cookie_central = Client(owner.base, owner.origin)
    client_cookie_central.cookie = tenant.cookie
    client_cookie_central.request("GET", "/api/auth/me", status=401)
    client_cookie_central.request("GET", "/api/sites", status=401)
    visitor.request("POST", "/extranet/api/auth/login", {**credentials, "password": secrets.token_urlsafe(32)}, status=401)
    tenant.request("POST", "/extranet/api/auth/logout", status=204)
    tenant.request("GET", "/extranet/api/auth/me", status=401)
    assert_session_cookie(tenant.request("POST", "/extranet/api/auth/login", credentials), cookie_name)
    ok("central and client sessions are distinct and client login verifies its credentials")

    visible = as_array(tenant.request("GET", "/extranet/api/sites"))
    require(len(visible) == 1 and visible[0].get("id") == primary_id, "Extranet exposed another client's site")
    for foreign in foreign_sites:
        foreign_url = "/extranet/api/sites/" + foreign["id"]
        for suffix in ["", "/contents", "/settings", "/dashboard", "/structure", "/template-preview"]:
            tenant.request("GET", foreign_url + suffix, status=403)
        tenant.request("POST", foreign_url + "/contents", {"kind": "page", "title": "Forbidden", "slug": "forbidden", "excerpt": "", "body": "Forbidden"}, status=403)
    for suffix in ["/extranet-access", "/domain-assistant"]:
        tenant.request("GET", client_url + suffix, status=404, response_format="none")
    tenant.request("POST", "/extranet/api/sites", {"name": "Forbidden", "slug": "forbidden", "domain": None}, status=405, response_format="none")
    tenant.request("PUT", client_url, {"name": "Forbidden", "slug": "forbidden", "domain": None}, status=405, response_format="none")
    ok("client administration exposes one site and rejects owner-only or foreign-site operations")

    visitor.request("GET", client_url + "/structure", status=401)
    visitor.request("GET", client_url + "/template-preview", status=401)
    structure = as_object(tenant.request("GET", client_url + "/structure"))
    require(structure.get("site_id") == primary_id, "Extranet page inventory resolved another site")
    preview = tenant.request("GET", client_url + "/template-preview?page=home&area=full", response_format="html")
    require("Site exemple" in preview.payload and "Client alpha" not in preview.payload, "Extranet template preview crossed tenant boundaries")
    require("frame-ancestors 'self'" in preview.headers.get("Content-Security-Policy", ""), "Authenticated preview cannot be embedded by its own extranet")
    require(preview.headers.get("Cache-Control") == "no-store", "Authenticated preview must not be cached")
    ok("client page inventory and template preview require the correct tenant session")

    page = {"kind": "page", "title": "Client extranet page", "slug": f"extranet-{nonce}", "excerpt": "Client summary", "body": "Client-managed content"}
    tenant.request("POST", client_url + "/contents", page, origin=None, status=403)
    tenant.request("POST", client_url + "/contents", page, origin=owner.origin, status=403)
    created = as_object(tenant.request("POST", client_url + "/contents", page, status=201))
    page_url = client_url + "/contents/" + created["id"]
    require(as_object(owner.request("GET", "/api/sites/" + primary_id + "/contents/" + created["id"])).get("body") == page["body"], "Client content was not stored on its own site")
    tenant.request("POST", page_url + "/publish")
    document = visitor.request_html("/pages/" + page["slug"])
    assert_public_metadata(document, page["title"], page["excerpt"], client_base + "/pages/" + page["slug"])
    tenant.request("PUT", page_url, {key: value for key, value in {**page, "body": "Edited client content"}.items() if key != "kind"})
    tenant.request("POST", page_url + "/publish")
    require("Edited client content" in visitor.request_html("/pages/" + page["slug"]), "Client publication did not update its public site")
    tenant.request("POST", page_url + "/unpublish")
    visitor.request_html("/pages/" + page["slug"], status=404)
    tenant.request("DELETE", page_url, status=204)
    tenant.request("GET", page_url, status=404)
    settings = as_object(tenant.request("GET", client_url + "/settings"))
    tenant.request("PUT", client_url + "/settings", {**settings, "description": "Updated from client extranet"})
    require(as_object(owner.request("GET", "/api/sites/" + primary_id + "/settings")).get("description") == "Updated from client extranet", "Client settings did not persist on its site")
    tenant.request("GET", client_url + "/dashboard")
    ok("client can edit, publish, unpublish and delete its own content with same-origin protection")

    support_link = as_object(owner.request("POST", central_url + "/session", {}))
    access = fragment_token(support_link.get("url"), "access")
    support = Client(client_base, client_base, cookie_name)
    visitor.request("POST", "/extranet/api/access", {"token": access}, origin=owner.origin, status=403)
    support_response = support.request("POST", "/extranet/api/access", {"token": access})
    assert_session_cookie(support_response, cookie_name)
    require(as_object(support_response).get("role") == "owner_support", "Owner support login does not identify its separate role")
    visitor.request("POST", "/extranet/api/access", {"token": access}, status=403)
    require(len(as_array(support.request("GET", "/extranet/api/sites"))) == 1, "Owner support session escaped the selected client site")
    support.request("GET", "/extranet/api/sites/" + foreign_sites[0]["id"], status=403)
    ok("owner opens a scoped client session through a single-use access link")

    colleague, colleague_credentials = team_role_checks(owner, tenant, primary_id, foreign_sites[0], nonce, ok)
    previous_cookie = tenant.cookie
    changed_password = secrets.token_urlsafe(32)
    owner.request("POST", central_url + "/password", {"password": changed_password}, status=204)
    colleague.request("GET", "/extranet/api/auth/me")
    colleague.request("POST", "/extranet/api/auth/login", colleague_credentials)
    tenant.request("GET", "/extranet/api/auth/me", status=401)
    support.request("GET", "/extranet/api/auth/me", status=401)
    visitor.request("POST", "/extranet/api/auth/login", credentials, status=401)
    tenant.request("POST", "/extranet/api/auth/login", {**credentials, "password": changed_password})
    stale = Client(client_base, client_base, cookie_name)
    stale.cookie = previous_cookie
    stale.request("GET", "/extranet/api/auth/me", status=401)
    previous_cookie = tenant.cookie
    tenant.request("POST", "/extranet/api/auth/logout", status=204)
    stale.cookie = previous_cookie
    stale.request("GET", "/extranet/api/auth/me", status=401)
    ok("owner password reset revokes client sessions and logout prevents cookie replay")


def starter_template_checks(owner: Client, visitor: Client, client_base: str, site_a_url: str, site_a_home: str, site_b_home: str, ok) -> None:
    """The shared template works for the initial site and later API-created sites."""
    client_visitor = Client(client_base, client_base)
    for reader, home, name in [(client_visitor, "", "Site exemple"), (visitor, site_a_home, "Client alpha"), (visitor, site_b_home, "Client beta")]:
        document = reader.request_html(home or "/")
        for suffix in ["/a-propos", "/blog", "/mon-compte"]:
            require('href="' + home + suffix + '"' in document, "Shared starter menu points outside its site")
        require('aria-label="Mon compte"' in document and '/assets/icons/user-round.svg' in document, "Account navigation must expose its accessible SVG icon")
        require(">Extranet<" not in document and ">Extranet " not in document, "Public navigation still exposes the old Extranet label")
        require(html.escape(name) in document and "Bienvenue chez" in document, "Shared starter does not use this client's identity")
        require("Le site est en cours de construction." in document and '/assets/site-construction.webp' in document, "New site did not receive the common construction template")
        reader.request_html(home + "/a-propos")
        blog = reader.request_html(home + "/blog")
        require("Les premières histoires se préparent." in blog, "Disabled or empty blog must provide a usable public state")
        require(as_array(owner.request("GET", site_a_url + "/contents")) == [], "Rendering a starter page must not create editable content")
    for asset in ["/assets/icons/user-round.svg", "/assets/icons/arrow-up-right.svg", "/assets/site-construction.webp"]:
        visitor.request("GET", asset, response_format="none")
    for suffix in ["/a-propos", "/blog"]:
        visitor.request_html("/s/nonexistent-starter-site" + suffix, status=404)
        client_visitor.request_html(site_a_home + suffix, status=404)
    ok("initial and future sites share working welcome, about, blog and account navigation")

    about_url = site_a_home + "/a-propos"
    fallback = visitor.request_html(about_url)
    require("Notre présentation prend forme." in fallback, "About route must have a starter state")
    page = {"kind": "page", "slug": "a-propos", "title": "Une présentation <script>sans exécution</script>", "excerpt": "À propos & identité", "body": "Notre présentation publiée.\n\n<script>texte de présentation</script>"}
    saved = as_object(owner.request("POST", site_a_url + "/contents", page, status=201))
    content_url = site_a_url + "/contents/" + saved["id"]
    require(page["body"] not in visitor.request_html(about_url), "About draft leaked into the public starter page")
    owner.request("POST", content_url + "/publish")
    rendered = visitor.request_html(about_url)
    assert_public_metadata(rendered, page["title"], page["excerpt"], owner.origin + about_url)
    require(html.escape(page["title"]) in rendered and "&lt;script&gt;texte de présentation&lt;/script&gt;" in rendered, "About snapshot was not rendered as escaped text")
    require("<script>sans exécution</script>" not in rendered, "About title was rendered as executable markup")
    require("Notre présentation publiée." not in visitor.request_html(site_b_home + "/a-propos"), "About snapshot crossed client sites")
    require("Le site est en cours de construction." not in visitor.request_html(site_a_home), "Published site must not be marked as under construction")
    alias = visitor.request_html(site_a_home + "/pages/a-propos")
    assert_public_metadata(alias, page["title"], page["excerpt"], owner.origin + about_url)
    owner.request("PUT", content_url, {key: value for key, value in {**page, "body": "About draft that must remain private"}.items() if key != "kind"})
    require("About draft that must remain private" not in visitor.request_html(about_url), "Editing an about draft changed the public snapshot")
    require("Notre présentation publiée." in visitor.request_html(about_url), "About snapshot disappeared before republication")
    owner.request("POST", content_url + "/unpublish")
    restored = visitor.request_html(about_url)
    require("Notre présentation prend forme." in restored and "Notre présentation publiée." not in restored, "Unpublished about page did not restore its safe starter state")
    owner.request("DELETE", content_url, status=204)
    ok("about page follows published snapshots, canonical alias and client isolation")


def site_structure_checks(owner: Client, visitor: Client, site_a_url: str, site_b_url: str, site_a_home: str, ok) -> None:
    """Inventory covers built-in pages and stored content without publishing drafts."""
    for suffix in ["/structure", "/template-preview"]:
        denied = visitor.request("GET", site_a_url + suffix, status=401)
        require("frame-ancestors 'none'" in denied.headers.get("Content-Security-Policy", ""), "Denied preview relaxed embedding protections")
        owner.request("GET", "/api/sites/00000000-0000-0000-0000-000000000000" + suffix, status=404)
    structure = as_object(owner.request("GET", site_a_url + "/structure"))
    pages = {page["id"]: page for page in structure["pages"]}
    require(set(pages) == {"home", "about", "blog", "account"}, "Empty site inventory is missing its built-in pages")
    require(pages["home"]["path"] == "/" and pages["about"]["path"] == "/a-propos" and pages["blog"]["path"] == "/blog" and pages["account"]["path"] == "/mon-compte", "Built-in page inventory contains incorrect routes")
    require(pages["blog"]["status"] == "disabled" and pages["account"]["status"] == "private", "Inventory confuses disabled modules and private account access with published content")
    require(structure["template"]["id"] == "welcome-v1", "Site inventory lost the common template identity")
    header = structure["template"]["header"]["links"]
    require([link["path"] for link in header] == ["/", "/a-propos", "/blog", "/mon-compte"], "Template menu differs from the public default menu")
    require(as_array(owner.request("GET", site_a_url + "/contents")) == [], "Reading site structure created pages in the database")

    for page in ["home", "about", "blog"]:
        for area in ["full", "header", "footer"]:
            response = owner.request("GET", site_a_url + f"/template-preview?page={page}&area={area}", response_format="html")
            require('data-template-preview="true"' in response.payload, "Preview response has no genuine preview marker")
            require("Client alpha" in response.payload and "Client beta" not in response.payload, "Preview uses another site identity")
            require("frame-ancestors 'self'" in response.headers.get("Content-Security-Policy", ""), "Preview embedding must be limited to its own origin")
            require(response.headers.get("Cache-Control") == "no-store" and "noindex" in response.headers.get("X-Robots-Tag", ""), "Private preview must neither be cached nor indexed")
            if area == "header":
                require('class="site-header"' in response.payload and 'class="site-footer"' not in response.payload and 'id="main"' not in response.payload, "Header preview includes unrelated regions")
            if area == "footer":
                require('class="site-footer"' in response.payload and 'class="site-header"' not in response.payload and 'id="main"' not in response.payload, "Footer preview includes unrelated regions")
    for query in ["page=unknown", "area=unknown", "page=home&area=../../admin"]:
        owner.request("GET", site_a_url + "/template-preview?" + query, status=400, response_format="none")
    public = visitor.request("GET", site_a_home, response_format="html")
    require("frame-ancestors 'none'" in public.headers.get("Content-Security-Policy", ""), "Adding preview weakened public page embedding protection")
    ok("all built-in pages and real template previews are authenticated, scoped and read-only")

    page = {"kind": "page", "title": "Inventory draft", "slug": "inventory-draft", "excerpt": "Private draft summary", "body": "INVENTORY_SECRET_DRAFT"}
    created = as_object(owner.request("POST", site_a_url + "/contents", page, status=201))
    content_url = site_a_url + "/contents/" + created["id"]
    structure = as_object(owner.request("GET", site_a_url + "/structure"))
    custom = [item for item in structure["pages"] if item["content_id"] == created["id"]]
    require(len(structure["pages"]) == 5 and len(custom) == 1 and custom[0]["status"] == "draft", "Stored draft page is absent or incorrectly marked published in the inventory")
    require("Inventory draft" not in owner.request_html(site_a_url + "/template-preview"), "Template preview exposed an unpublished page")
    require(len(as_object(owner.request("GET", site_b_url + "/structure"))["pages"]) == 4, "Page inventory crossed site boundaries")
    owner.request("POST", content_url + "/publish")
    structure = as_object(owner.request("GET", site_a_url + "/structure"))
    require(any(item["content_id"] == created["id"] and item["status"] == "published" for item in structure["pages"]), "Published page status did not reach the inventory")
    require(any(link["path"] == "/pages/inventory-draft" for link in structure["template"]["header"]["links"]), "Published page did not appear in the real template menu")
    owner.request("PUT", content_url, {key: value for key, value in {**page, "title": "PRIVATE_RENAMED_ABOUT", "slug": "a-propos"}.items() if key != "kind"})
    structure = as_object(owner.request("GET", site_a_url + "/structure"))
    about = next(item for item in structure["pages"] if item["id"] == "about")
    require(about["content_id"] == created["id"] and about["status"] == "automatic", "About draft did not preserve the automatic public page and its edit target")
    require(any(item.get("published_path") == "/pages/inventory-draft" for item in structure["pages"]), "Renaming a draft lost the still-published page URL")
    require("PRIVATE_RENAMED_ABOUT" not in owner.request_html(site_a_url + "/template-preview?page=about"), "About preview exposed the draft instead of the public page")
    owner.request("POST", content_url + "/publish")
    structure = as_object(owner.request("GET", site_a_url + "/structure"))
    require(len(structure["pages"]) == 4, "Published About page was duplicated in the built-in inventory")
    about = next(item for item in structure["pages"] if item["id"] == "about")
    require(about["status"] == "published" and about["content_id"] == created["id"], "About page inventory lost its published editor target")
    require("PRIVATE_RENAMED_ABOUT" in owner.request_html(site_a_url + "/template-preview?page=about"), "About preview did not render the genuine published snapshot")
    owner.request("DELETE", content_url, status=204)
    ok("page inventory follows draft edits, live URLs, About overrides and actual navigation")




def team_role_checks(owner: Client, tenant: Client, site_id: str, foreign_site: dict, nonce: str, ok):
    path = "/extranet/api/sites/" + site_id
    central = "/api/sites/" + site_id
    snapshot = as_object(tenant.request("GET", path + "/users"))
    primary = snapshot["users"][0]
    require(primary["role"] == "admin" and primary["is_primary"] and primary["active"], "Existing client must remain the primary administrator")
    tenant.request("PUT", path + "/users/" + primary["id"], {"role": "viewer", "active": True}, status=409)
    tenant.request("PUT", path + "/users/" + primary["id"], {"role": "admin", "active": False}, status=409)
    tenant.request("GET", "/extranet/api/sites/" + foreign_site["id"] + "/users", status=403)
    members = {}
    for role in ["editor", "viewer", "admin"]:
        email = role + "-" + nonce + "@example.test"
        invitation = as_object(tenant.request("POST", path + "/users/invitations", {"email": email, "name": role.title(), "role": role}))
        url = urllib.parse.urlsplit(invitation["activation_url"])
        require(url.scheme + "://" + url.netloc == tenant.origin and url.path == "/extranet", "Team invitation left its client domain")
        token = urllib.parse.parse_qs(url.fragment)["invitation"][0]
        member = Client(tenant.base, tenant.origin, "__Host-extranet_session")
        details = as_object(member.request("POST", "/extranet/api/team-invitation/inspect", {"token": token}))
        require(details["email"] == email and details["role"] == role, "Invitation identity is not bound to its email and role")
        creds = {"email": email, "password": secrets.token_urlsafe(32)}
        accepted = member.request("POST", "/extranet/api/team-invitation", {"token": token, "name": role.title(), "password": creds["password"]}, status=201)
        assert_session_cookie(accepted, "__Host-extranet_session")
        identity = as_object(accepted)
        require(identity["role"] == role and identity["site_id"] == site_id, "Accepted invitation granted another role or site")
        member.request("POST", "/extranet/api/team-invitation", {"token": token, "name": role.title(), "password": creds["password"]}, status=403)
        members[role] = (member, identity, creds)
    ok("site administrators invite three roles without losing the last active administrator")

    content = {"kind": "page", "title": "Team role test", "slug": "role-" + nonce, "excerpt": "", "body": "Editable by editors"}
    editor, editor_id, editor_creds = members["editor"]
    viewer, viewer_id, _ = members["viewer"]
    for member in [editor, viewer]:
        member.request("GET", path + "/users", status=403)
        member.request("POST", path + "/users/invitations", {"email": "forbidden@example.test", "name": "Forbidden", "role": "admin"}, status=403)
        member.request("PUT", path + "/settings", {"site_name": "Forbidden"}, status=403)
        member.request("GET", path + "/structure")
    created = as_object(editor.request("POST", path + "/contents", content, status=201))
    content_url = path + "/contents/" + created["id"]
    viewer.request("GET", content_url)
    viewer.request("POST", path + "/contents", content, status=403)
    viewer.request("PUT", content_url, {key: value for key,value in content.items() if key != "kind"}, status=403)
    viewer.request("POST", content_url + "/publish", status=403)
    viewer.request("DELETE", content_url, status=403)
    editor.request("POST", content_url + "/publish")
    editor.request("POST", content_url + "/unpublish")
    editor.request("DELETE", content_url, status=204)
    document = visual_document("Permission preview")
    for member in [editor, viewer]:
        member.request("POST", path + "/design/home/preview", {"document": document}, response_format="html")
        member.request("PUT", path + "/design/template", {"expected_revision": 0, "document": {}}, status=403)
    viewer.request("PUT", path + "/design/home", {"expected_revision": 0, "document": document}, status=403)
    editor.request("PUT", path + "/design/home", {"expected_revision": 0, "document": document})
    viewer.request("POST", path + "/design/home/publish", {"expected_revision": 1}, status=403)
    editor.request("POST", path + "/design/home/publish", {"expected_revision": 1})
    tenant.request("PUT", path + "/users/" + editor_id["id"], {"role": "viewer", "active": True}, status=204)
    require(as_object(editor.request("GET", "/extranet/api/auth/me"))["role"] == "viewer", "Role changes require an unsafe stale session refresh")
    editor.request("POST", path + "/contents", content, status=403)
    tenant.request("PUT", path + "/users/" + editor_id["id"], {"role": "editor", "active": True}, status=204)
    tenant.request("PUT", path + "/users/" + viewer_id["id"], {"role": "viewer", "active": False}, status=204)
    viewer.request("GET", "/extranet/api/auth/me", status=401)
    ok("editor and read-only permissions are enforced by the API and changes apply immediately")

    admin, admin_id, _ = members["admin"]
    pending = as_object(admin.request("POST", path + "/users/invitations", {"email": "revoked-" + nonce + "@example.test", "role": "viewer"}))
    token = urllib.parse.parse_qs(urllib.parse.urlsplit(pending["activation_url"]).fragment)["invitation"][0]
    tenant.request("PUT", path + "/users/" + admin_id["id"], {"role": "editor", "active": True}, status=204)
    admin.request("POST", "/extranet/api/team-invitation/inspect", {"token": token}, status=403)
    changed = secrets.token_urlsafe(32)
    owner.request("POST", central + "/users/" + editor_id["id"] + "/password", {"password": changed}, status=204)
    editor.request("GET", "/extranet/api/auth/me", status=401)
    editor.request("POST", "/extranet/api/auth/login", editor_creds, status=401)
    editor_creds["password"] = changed
    editor.request("POST", "/extranet/api/auth/login", editor_creds)
    tenant.request("GET", "/extranet/api/auth/me")
    ok("revoked invitation privileges and targeted owner password resets affect only their intended accounts")
    return editor, editor_creds


def visual_document(title: str) -> dict:
    common = {"align": "left", "spacing": "medium", "theme": "paper"}
    return {"version": 1, "sections": [
        {**common, "id": str(uuid.uuid4()), "type": "hero", "eyebrow": "Bienvenue", "title": title, "text": "A visual introduction", "image_url": None, "image_alt": "", "button": {"label": "Mon compte", "href": "/mon-compte"}},
        {**common, "id": str(uuid.uuid4()), "type": "text", "title": "Second section", "text": "Text <script>must-not-execute</script>"},
        {**common, "id": str(uuid.uuid4()), "type": "image", "url": "/assets/site-construction.webp", "alt": "Site under construction", "caption": "Local illustration"},
        {**common, "id": str(uuid.uuid4()), "type": "columns", "title": "Two columns", "items": [{"title": "First column", "text": "First text"},{"title": "Second column", "text": "Second text"}]},
        {**common, "id": str(uuid.uuid4()), "type": "cta", "title": "Join us", "text": "Personal account", "button": {"label": "Create my account", "href": "/mon-compte"}},
    ]}


def builder_checks(owner: Client, guest: Client, site_url: str, site_home: str, other_url: str, other_home: str, nonce: str, ok) -> None:
    home_url = site_url + "/design/home"
    template_url = site_url + "/design/template"
    guest.request("GET", home_url, status=401)
    default = as_object(owner.request("GET", home_url))
    require(default["revision"] == 0 and default["published_revision"] is None, "Opening a default visual homepage stored or published it")
    require(as_array(owner.request("GET", site_url + "/contents")) == [], "Builder opening created unexpected content")
    document = visual_document("Visual homepage " + nonce)
    preview = owner.request("POST", home_url + "/preview", {"document": document}, response_format="html")
    require(document["sections"][0]["title"] in preview.payload and 'data-section-id="' in preview.payload, "Builder preview is missing its real section content")
    require("<script>must-not-execute</script>" not in preview.payload and "&lt;script&gt;must-not-execute&lt;/script&gt;" in preview.payload, "Builder preview interpreted user markup")
    require(preview.headers.get("Cache-Control") == "no-store" and "noindex" in preview.headers.get("X-Robots-Tag", ""), "Draft preview is cacheable or indexable")
    require("script-src 'none'" in preview.headers.get("Content-Security-Policy", ""), "Draft preview allows executable scripts")
    require(as_object(owner.request("GET", home_url))["revision"] == 0, "Unsaved preview modified the stored document")
    require(document["sections"][0]["title"] not in guest.request_html(site_home), "Unsaved preview leaked into the public homepage")
    saved = as_object(owner.request("PUT", home_url, {"expected_revision": 0, "document": document}))
    require(saved["revision"] == 1 and saved["has_unpublished_changes"], "Saving visual draft did not persist a private revision")
    require(document["sections"][0]["title"] not in guest.request_html(site_home), "Saved draft became public without publication")
    owner.request("PUT", home_url, {"expected_revision": 0, "document": document}, status=409)
    owner.request("POST", home_url + "/publish", {"expected_revision": 0}, status=409)
    published = as_object(owner.request("POST", home_url + "/publish", {"expected_revision": 1}))
    require(published["published_revision"] == 1 and not published["has_unpublished_changes"], "Visual homepage publication has an inconsistent revision")
    live = guest.request_html(site_home)
    require(document["sections"][0]["title"] in live and "Second section" in live and "Two columns" in live, "Published homepage did not render its blocks")
    reordered = {"version": 1, "sections": list(reversed(document["sections"]))}
    owner.request("PUT", home_url, {"expected_revision": 1, "document": reordered})
    require(guest.request_html(site_home).index('data-section-id="' + document["sections"][0]["id"]) < guest.request_html(site_home).index('data-section-id="' + document["sections"][4]["id"]), "Reordering a draft changed the published section order")
    owner.request("POST", home_url + "/publish", {"expected_revision": 2})
    live = guest.request_html(site_home)
    require(live.index('data-section-id="' + document["sections"][4]["id"]) < live.index('data-section-id="' + document["sections"][0]["id"]), "Published section order differs from the saved document")
    require(document["sections"][0]["title"] not in guest.request_html(other_home), "Visual homepage leaked into another site")
    ok("visual pages preview, save, reorder and publish with private drafts and revision conflicts")

    for bad in [
        {"version": 2, "sections": document["sections"]},
        {"version": 1, "sections": [document["sections"][0], document["sections"][0]]},
        {"version": 1, "sections": [{**document["sections"][1], "type": "html"}]},
        {"version": 1, "sections": [{**document["sections"][2], "url": "javascript:alert(1)"}]},
        {"version": 1, "sections": [{**document["sections"][4], "button": {"label": "Bad", "href": "javascript:alert(1)"}}]},
    ]:
        owner.request("PUT", home_url, {"expected_revision": 2, "document": bad}, status=422)
    require(as_object(owner.request("GET", home_url))["revision"] == 2, "Invalid builder document changed the stored revision")
    ok("builder rejects invalid document versions, duplicated blocks, raw HTML and unsafe URLs")

    page = {"kind": "page", "title": "Legacy to visual", "slug": "visual-" + nonce, "excerpt": "Legacy summary", "body": "Legacy body preserved"}
    created = as_object(owner.request("POST", site_url + "/contents", page, status=201))
    page_url = site_url + "/contents/" + created["id"]
    layout_url = page_url + "/layout"
    layout = as_object(owner.request("GET", layout_url))
    require("Legacy body preserved" in json.dumps(layout["document"]), "Opening a legacy page discarded its existing text")
    owner.request("GET", other_url + "/contents/" + created["id"] + "/layout", status=404)
    page_document = visual_document("Custom visual page " + nonce)
    layout_saved = as_object(owner.request("PUT", layout_url, {"expected_revision": layout["revision"], "document": page_document}))
    guest.request_html(site_home + "/pages/" + page["slug"], status=404)
    owner.request("POST", page_url + "/publish", {"expected_revision": layout["revision"]}, status=409)
    owner.request("POST", page_url + "/publish", {"expected_revision": layout_saved["revision"]})
    live_page = guest.request_html(site_home + "/pages/" + page["slug"])
    require(page_document["sections"][0]["title"] in live_page, "Published custom page ignored the visual layout")
    page_document["sections"][0]["title"] = "Private changed hero " + nonce
    updated_layout = as_object(owner.request("PUT", layout_url, {"expected_revision": layout_saved["revision"], "document": page_document}))
    require("Private changed hero" not in guest.request_html(site_home + "/pages/" + page["slug"]), "Custom page layout draft leaked into publication")
    inventory = as_object(owner.request("GET", site_url + "/structure"))
    require(next(p for p in inventory["pages"] if p.get("content_id") == created["id"])["has_unpublished_changes"], "Page inventory did not report visual draft changes")
    owner.request("POST", page_url + "/publish", {"expected_revision": updated_layout["revision"]})
    owner.request("POST", page_url + "/unpublish", {"expected_revision": layout_saved["revision"]}, status=409)
    require("Private changed hero " + nonce in guest.request_html(site_home + "/pages/" + page["slug"]), "A stale editor removed a newer visual publication")
    require(as_object(owner.request("GET", layout_url))["revision"] == updated_layout["revision"], "Rejected stale unpublish changed the stored revision")
    owner.request("POST", page_url + "/unpublish", status=409)
    ok("legacy content opens in the builder without data loss and visual page publications stay atomic")
    ok("stale or missing revisions cannot withdraw a newer visual publication")

    template = as_object(owner.request("GET", template_url))
    template_doc = template["document"]
    template_doc["header"]["model"] = "centered"
    template_doc["footer"]["model"] = "columns"
    template_doc["footer"]["description"] = "Shared footer " + nonce
    template_doc["header"]["links"].insert(0, {"id": str(uuid.uuid4()), "label": "Visual menu page", "target": {"kind": "page", "id": created["id"]}})
    template_saved = as_object(owner.request("PUT", template_url, {"expected_revision": 0, "document": template_doc}))
    require("Shared footer " + nonce not in guest.request_html(site_home), "Template draft changed the public site")
    owner.request("POST", template_url + "/publish", {"expected_revision": template_saved["revision"]})
    for path in [site_home, site_home + "/a-propos", site_home + "/blog", site_home + "/pages/" + page["slug"]]:
        rendered = guest.request_html(path)
        require("Shared footer " + nonce in rendered and "Visual menu page" in rendered, "Published shared template is missing on a site page")
        require('href="' + site_home + '/mon-compte"' in rendered, "Custom template account link points to the team extranet")
    require("Shared footer " + nonce not in guest.request_html(other_home), "Shared template changed another client site")
    bad_template = json.loads(json.dumps(template_doc))
    foreign = as_object(owner.request("POST", other_url + "/contents", {**page,"title":"Foreign page"}, status=201))
    bad_template["header"]["links"][0]["target"]["id"] = foreign["id"]
    owner.request("PUT", template_url, {"expected_revision": template_saved["revision"], "document": bad_template}, status=422)
    owner.request("POST", page_url + "/unpublish", {"expected_revision": updated_layout["revision"]})
    owner.request("POST", page_url + "/publish", {"expected_revision": updated_layout["revision"]}, status=409)
    require("Visual menu page" not in guest.request_html(site_home), "Unpublished page remains reachable through a published custom menu")
    owner.request("POST", template_url + "/publish", {"expected_revision": template_saved["revision"]}, status=422)
    owner.request("DELETE", page_url, status=204)
    owner.request("DELETE", other_url + "/contents/" + foreign["id"], status=204)
    ok("shared header and footer publish consistently with same-site menus and no links to private pages")

def builder_widget_checks(owner: Client, guest: Client, site_url: str, site_home: str, other_home: str, nonce: str, ok) -> None:
    """Exercise the widget/style contract through storage and public rendering."""
    page = as_object(owner.request("POST", site_url + "/contents", {
        "kind": "page", "title": "Widget styling", "slug": "widgets-" + nonce,
        "excerpt": "Responsive widgets", "body": "Original content preserved",
    }, status=201))
    page_url = site_url + "/contents/" + page["id"]
    layout_url = page_url + "/layout"
    public_url = site_home + "/pages/" + page["slug"]
    current = as_object(owner.request("GET", layout_url))
    common = {"align": "right", "spacing": "small", "theme": "paper"}
    title = "Styled heading " + nonce
    style = {
        "background": "purple", "text_color": "white", "font_family": "serif",
        "font_weight": "bold", "border_radius": 24, "border_width": 2, "border_color": "purple",
        "desktop": {"title_size": 64, "text_size": 20, "content_width": 960,
                    "padding": {"top": 48, "right": 32, "bottom": 48, "left": 32}},
        "tablet": {"title_size": 40, "align": "center"},
        "mobile": {"title_size": 28, "align": "left", "content_width": 0,
                   "padding": {"top": 24, "right": 16, "bottom": 24, "left": 16},
                   "margin": {"top": 8, "right": 0, "bottom": 8, "left": 0}},
        "hidden_desktop": False, "hidden_tablet": True, "hidden_mobile": False,
    }
    widgets = {"version": 1, "sections": [
        {**common, "type": "heading", "id": str(uuid.uuid4()), "title": title, "style": style},
        {**common, "type": "button", "id": str(uuid.uuid4()), "button": {"label": "Visit my account", "href": "/mon-compte"}},
        {**common, "type": "divider", "id": str(uuid.uuid4())},
        {**common, "type": "spacer", "id": str(uuid.uuid4()), "height": 64},
    ]}
    preview = owner.request("POST", layout_url + "/preview", {"document": widgets}, response_format="html")
    require(title in preview.payload, "New heading widget is missing from the preview")
    for kind in ["heading", "button", "divider", "spacer"]:
        require('data-section-type="' + kind + '"' in preview.payload, "A basic widget is missing from the preview")
    require('style="' not in preview.payload and "<style" not in preview.payload, "Widget styles bypass the external stylesheet policy")
    require("style-src 'self'" in preview.headers.get("Content-Security-Policy", "") and "unsafe-inline" not in preview.headers.get("Content-Security-Policy", ""), "Widget styles weakened the preview policy")
    saved = as_object(owner.request("PUT", layout_url, {"expected_revision": current["revision"], "document": widgets}))
    require(saved["document"]["sections"][0]["style"] == style, "Responsive style values did not round-trip")
    guest.request_html(public_url, status=404)
    owner.request("POST", page_url + "/publish", {"expected_revision": saved["revision"]})
    live = guest.request_html(public_url)
    require(title in live and 'href="' + site_home + '/mon-compte"' in live, "New widgets lost content or scoped button destinations")
    for css_class in ["bs-bg-purple", "bs-text-white", "bs-font-serif", "bs-weight-bold", "bs-radius-24", "bs-border-2", "bs-border-color-purple", "bs-d-title-64", "bs-t-title-40", "bs-m-title-28", "bs-d-width-960", "bs-m-pt-24", "bs-m-mt-8", "bs-hide-tablet"]:
        require(css_class in live, "Published widgets lost an appearance or responsive setting")
    require(title not in guest.request_html(other_home), "Styled widgets leaked to another tenant")
    ok("new heading, button, divider and spacer widgets persist and publish with scoped links")

    styles_before = live
    changed = json.loads(json.dumps(widgets))
    changed["sections"][0]["title"] = "Unpublished styled heading " + nonce
    changed["sections"][0]["style"]["background"] = "blue"
    changed_saved = as_object(owner.request("PUT", layout_url, {"expected_revision": saved["revision"], "document": changed}))
    require(guest.request_html(public_url) == styles_before, "Changing widget styles in a draft changed the public snapshot")
    for invalid_style in [
        {"background": "url(https://invalid.example/tracker)"},
        {"text_color": "red;position:fixed"},
        {"desktop": {"title_size": 9999}},
        {"mobile": {"padding": {"top": -1, "right": 0, "bottom": 0, "left": 0}}},
        {"mobile": {"margin": {"top": 13, "right": 0, "bottom": 0, "left": 0}}},
        {"desktop": {"content_width": 20000}},
        {"border_radius": 2000}, {"css": "body{display:none}"},
    ]:
        invalid = json.loads(json.dumps(widgets))
        invalid["sections"][0]["style"] = invalid_style
        owner.request("PUT", layout_url, {"expected_revision": changed_saved["revision"], "document": invalid}, status=422)
    invalid = json.loads(json.dumps(widgets))
    invalid["sections"][3]["height"] = 9999
    owner.request("PUT", layout_url, {"expected_revision": changed_saved["revision"], "document": invalid}, status=422)
    require(as_object(owner.request("GET", layout_url))["revision"] == changed_saved["revision"], "Rejected styles changed the document revision")
    require(guest.request_html(public_url) == styles_before, "Rejected styles changed a public page")
    owner.request("POST", page_url + "/publish", {"expected_revision": changed_saved["revision"]})
    require(changed["sections"][0]["title"] in guest.request_html(public_url), "Updated widget snapshot did not publish")
    owner.request("DELETE", page_url, status=204)
    ok("responsive widget styles remain private until publication and reject arbitrary CSS or unsupported values")


def visitor_account_checks(owner: Client, client_base: str, primary: dict, foreign_site: dict, nonce: str, ok) -> None:
    """Visitor identity stays separate from the team and from every other tenant."""
    visitor = Client(client_base, client_base, "__Host-visitor_session")
    guest = Client(client_base, client_base, "__Host-visitor_session")
    api = "/mon-compte/api"
    for path in ["/mon-compte", "/mon-compte/"]:
        shell = guest.request("GET", path, response_format="html")
        require(shell.headers.get("Location") is None, "Visitor page redirected off the client's domain")
        require(shell.headers.get("Cache-Control") == "no-store", "Visitor page must not be cached")
        require("account.js" in shell.payload and "admin.js" not in shell.payload, "Visitor page uses the administrative shell")
    guest.request("GET", "/mon-compte", extra_headers={"Host": "unknown.example.test"}, status=404)
    guest.request("GET", api + "/me", status=401)
    boot = as_object(guest.request("GET", api + "/bootstrap"))
    require(boot["site"]["id"] == primary["id"] and boot["registration_open"] is True, "Visitor account did not select the correct site")
    data = {"name": "Visitor example", "email": f"visitor-{nonce}@example.test", "password": secrets.token_urlsafe(32)}
    guest.request("POST", api + "/register", data, origin=None, status=403)
    guest.request("POST", api + "/register", data, origin=owner.origin, status=403)
    guest.request("POST", api + "/register", data, content_type="text/plain", status=415)
    guest.request("POST", api + "/register", {**data, "password": "short"}, status=422)
    created = visitor.request("POST", api + "/register", data, status=201)
    assert_session_cookie(created, "__Host-visitor_session")
    identity = as_object(created)
    require(identity["email"] == data["email"] and identity["site_id"] == primary["id"], "Visitor registration has the wrong identity or tenant")
    require(not any(k in identity for k in ["password", "password_hash", "role", "token"]), "Visitor response exposes a secret or grants an administrative role")
    guest.request("POST", api + "/register", data, status=409)
    require(as_object(visitor.request("GET", api + "/me"))["id"] == identity["id"], "Visitor session lost its own identity")
    ok("free visitor signup is distinct, same-origin and uses its own secure session")

    for name, cookie in [("central", owner.cookie), ("visitor", visitor.cookie)]:
        isolated = Client(client_base, client_base)
        isolated.cookie = cookie
        if name == "central":
            isolated.request("GET", api + "/me", status=401)
        else:
            isolated.request("GET", "/api/auth/me", status=401)
            isolated.request("GET", "/extranet/api/auth/me", status=401)
            isolated.request("GET", "/extranet/api/sites/" + primary["id"] + "/users", status=401)
    edited = as_object(visitor.request("PUT", api + "/profile", {"name": "Updated visitor"}))
    require(edited["name"] == "Updated visitor" and edited["email"] == data["email"], "Visitor profile update changed another field")
    require(as_object(visitor.request("GET", api + "/me"))["name"] == "Updated visitor", "Visitor profile did not persist")
    guest.request("POST", api + "/login", {"email": data["email"], "password": "incorrect-long-password"}, status=401)
    saved_cookie = visitor.cookie
    visitor.request("POST", api + "/logout", status=204)
    guest.cookie = saved_cookie
    guest.request("GET", api + "/me", status=401)
    guest.cookie = ""
    login = visitor.request("POST", api + "/login", {"email": data["email"], "password": data["password"]})
    assert_session_cookie(login, "__Host-visitor_session")
    require(login.headers.get("Cache-Control") == "no-store", "Visitor API must not be cached")
    ok("visitor profile, login and logout work without administrative access")

    # This fixture is limited to the named disposable Docker project. A .test
    # Host header exercises production tenant routing over a loopback socket.
    require(os.environ.get("CMS_TEST_DB_FIXTURES") == "1", "Enable disposable database fixtures for tenant checks")
    site_id = str(uuid.UUID(foreign_site["id"]))
    fixture = "INSERT INTO cms_domain_bindings(domain,site_id,status,verified_at,connected_at) VALUES ('second-client.example.test','" + site_id + "','connected',now(),now());"
    result = subprocess.run(["docker", "compose", "-f", "infra/compose.test.yaml", "exec", "-T", "postgres", "psql", "-v", "ON_ERROR_STOP=1", "-U", "cms_test", "-d", "cms_test", "-c", fixture], capture_output=True)
    require(result.returncode == 0, "Disposable tenant routing fixture failed")
    second = Client(client_base, "https://second-client.example.test", "__Host-visitor_session")
    second.cookie = visitor.cookie
    headers = {"Host": "second-client.example.test"}
    second.request("GET", api + "/me", extra_headers=headers, status=401)
    second.cookie = ""
    second.request("POST", api + "/login", {"email": data["email"], "password": data["password"]}, extra_headers=headers, status=401)
    foreign_identity = as_object(second.request("POST", api + "/register", data, extra_headers=headers, status=201))
    require(foreign_identity["site_id"] == site_id and foreign_identity["id"] != identity["id"], "Same email merged two tenant visitor accounts")
    require(as_object(second.request("GET", api + "/me", extra_headers=headers))["site_id"] == site_id, "Second host selected the wrong account")
    second.request("GET", api + "/me", status=401)
    require(as_object(visitor.request("GET", api + "/me"))["name"] == "Updated visitor", "Another tenant changed the first visitor")
    visitor.request("POST", api + "/logout", status=204)
    ok("visitor cookies, credentials and profiles remain isolated between bound client domains")
    template_url = "/api/sites/" + primary["id"] + "/design/template"
    template = as_object(owner.request("GET", template_url))
    template["document"]["footer"]["description"] = "Visitor shared footer " + nonce
    template["document"]["footer"]["model"] = "columns"
    saved = as_object(owner.request("PUT", template_url, {"expected_revision": template["revision"], "document": template["document"]}))
    owner.request("POST", template_url + "/publish", {"expected_revision": saved["revision"]})
    require("Visitor shared footer " + nonce in visitor.request_html("/mon-compte"), "Visitor account ignored the published shared site template")
    ok("visitor account page shares the site's published header and footer")

def main() -> None:
    require(os.environ.get("CMS_TEST_ALLOW_SETUP") == "1", "Set CMS_TEST_ALLOW_SETUP=1 for a disposable test database")
    base = os.environ.get("CMS_TEST_BASE_URL", "").rstrip("/")
    parsed_base = urllib.parse.urlsplit(base)
    require(parsed_base.scheme in {"http", "https"}, "CMS_TEST_BASE_URL must be an HTTP(S) loopback URL")
    require(not parsed_base.username and not parsed_base.password, "Credentials must not be embedded in the test URL")
    require(not parsed_base.path and not parsed_base.query and not parsed_base.fragment, "Test base URL must contain only an origin")
    hostname = parsed_base.hostname or ""
    if hostname == "localhost":
        loopback = True
    else:
        try:
            loopback = ipaddress.ip_address(hostname).is_loopback
        except ValueError:
            loopback = False
    require(loopback, "Refusing to test a non-loopback host; never run this script against production")
    client_base = os.environ.get("CMS_TEST_CLIENT_BASE_URL", base).rstrip("/")
    parsed_client_base = urllib.parse.urlsplit(client_base)
    try:
        client_loopback = parsed_client_base.hostname == "localhost" or ipaddress.ip_address(parsed_client_base.hostname or "").is_loopback
    except ValueError:
        client_loopback = False
    require(
        client_loopback and parsed_client_base.scheme in {"http", "https"}
        and not parsed_client_base.username and not parsed_client_base.password
        and not parsed_client_base.path and not parsed_client_base.query and not parsed_client_base.fragment,
        "CMS_TEST_CLIENT_BASE_URL must be a loopback origin for the disposable CMS",
    )
    origin = os.environ.get("CMS_TEST_ORIGIN", base)
    parsed_origin = urllib.parse.urlsplit(origin)
    require(
        parsed_origin.scheme in {"http", "https"}
        and bool(parsed_origin.netloc)
        and not parsed_origin.path
        and not parsed_origin.query
        and not parsed_origin.fragment
        and not parsed_origin.username
        and not parsed_origin.password,
        "CMS_TEST_ORIGIN must contain only the configured public origin",
    )
    setup_token = os.environ.get("CMS_TEST_SETUP_TOKEN", "")
    require(bool(setup_token), "CMS_TEST_SETUP_TOKEN must be supplied privately")
    outsider = Client(base, origin)
    owner = Client(base, origin)
    nonce = secrets.token_hex(6)
    email = f"integration-{nonce}@example.test"
    password = secrets.token_urlsafe(32)
    login = {"email": email, "password": password}
    setup = {"token": setup_token, **login, "name": "Integration owner", "site_name": "Site exemple"}
    passed = 0

    def ok(label: str) -> None:
        nonlocal passed
        passed += 1
        print(f"PASS {label}", flush=True)

    bootstrap = as_object(outsider.request("GET", "/api/bootstrap"))
    require(bootstrap.get("initialized") is False, "Refusing to modify an initialized CMS; use a fresh disposable database")
    ok("fresh disposable CMS")
    initial_home = outsider.request_html("/")
    require("<title>" in initial_home and "/admin" in initial_home and "noindex" in initial_home, "Fresh CMS must display a usable, non-indexable welcome page before setup")
    outsider.request_html(f"/s/missing-{nonce}", status=404)
    ok("public welcome works before setup and unknown sites return HTTP 404")

    unknown_site = "/api/sites/00000000-0000-0000-0000-000000000000"
    for path in ["/api/sites", unknown_site + "/contents", unknown_site + "/settings", unknown_site + "/dashboard", "/api/auth/me"]:
        outsider.request("GET", path, status=401)
    ok("administration requires authentication")

    outsider.request("POST", "/api/setup", setup, origin=None, status=403)
    outsider.request("POST", "/api/setup", setup, origin="https://attacker.invalid", status=403)
    outsider.request("POST", "/api/setup", {**setup, "token": secrets.token_urlsafe(48)}, status=403)
    require(as_object(outsider.request("GET", "/api/bootstrap")).get("initialized") is False, "Rejected setup must not initialize the CMS")
    ok("activation token and Origin protect initial setup")

    created_owner_response = owner.request("POST", "/api/setup", setup, status=201)
    created_owner = as_object(created_owner_response)
    require(created_owner.get("email") == email, "Initial owner email differs")
    require("password" not in created_owner and "token" not in created_owner, "Setup response exposes credentials")
    assert_session_cookie(created_owner_response)
    require(as_object(outsider.request("GET", "/api/bootstrap")).get("initialized") is True, "Setup must mark the CMS initialized")
    outsider.request("POST", "/api/setup", setup, status=409)
    ok("one-time owner creation and secure session cookie")

    owner.request("POST", "/api/auth/logout", status=204)
    owner.request("GET", "/api/auth/me", status=401)
    outsider.request("POST", "/api/auth/login", {**login, "password": "incorrect-password-for-test"}, status=401)
    logged_in = owner.request("POST", "/api/auth/login", login)
    assert_session_cookie(logged_in)
    require(as_object(owner.request("GET", "/api/auth/me")).get("id") == created_owner.get("id"), "Authenticated owner differs from created owner")
    ok("login verifies credentials and establishes owner identity")


    initial_sites = as_array(owner.request("GET", "/api/sites"))
    require(len(initial_sites) == 1 and initial_sites[0].get("slug") == "site-exemple", "Setup must create the initial site from its name")
    primary_redirect = outsider.request("GET", "/s/site-exemple/extranet", status=303)
    require(primary_redirect.headers.get("Location") == client_base + "/extranet", "Primary named extranet must use the configured client origin")
    account_redirect = outsider.request("GET", "/s/site-exemple/mon-compte", status=303)
    require(account_redirect.headers.get("Location") == client_base + "/mon-compte", "Named visitor account must use the client origin")
    ok("named extranet and visitor account open their client domain")
    site_a_payload = {"name": "Client alpha", "slug": f"alpha-{nonce}", "domain": None}
    site_b_payload = {"name": "Client beta", "slug": f"beta-{nonce}", "domain": f"beta-{nonce}.example.test"}
    owner.request("POST", "/api/sites", site_a_payload, origin=None, status=403)
    owner.request("POST", "/api/sites", site_a_payload, origin="https://attacker.invalid", status=403)
    site_a = as_object(owner.request("POST", "/api/sites", site_a_payload, status=201))
    site_b = as_object(owner.request("POST", "/api/sites", site_b_payload, status=201))
    require(site_a.get("id") and site_b.get("id") and site_a["id"] != site_b["id"], "Sites must have distinct identifiers")
    owner.request("POST", "/api/sites", site_a_payload, status=409)
    require(len(as_array(owner.request("GET", "/api/sites"))) == 3, "Site list must contain the initial site and both test clients")
    site_a_url = "/api/sites/" + urllib.parse.quote(str(site_a["id"]), safe="")
    site_b_url = "/api/sites/" + urllib.parse.quote(str(site_b["id"]), safe="")
    public_a = "/api/public/sites/" + site_a_payload["slug"]
    public_b = "/api/public/sites/" + site_b_payload["slug"]
    site_a_home = "/s/" + site_a_payload["slug"]
    for site_slug in [site_a_payload["slug"], site_b_payload["slug"]]:
        for suffix in ["/extranet", "/extranet/", "/mon-compte", "/mon-compte/"]:
            outsider.request("GET", "/s/" + site_slug + suffix, status=404)
    outsider.request("GET", "/s/missing-client/extranet", status=404)
    require('href="' + site_a_home + '/mon-compte"' in outsider.request_html(site_a_home), "Public site is missing its visitor account link")
    ok("unconnected and unknown client extranets cannot fall back to the central administration")
    updated_site = as_object(owner.request("PUT", site_a_url, {**site_a_payload, "name": "Client alpha updated"}))
    require(updated_site.get("name") == "Client alpha updated", "Site metadata changes were not saved")
    require(as_object(owner.request("GET", site_b_url)).get("domain") == site_b_payload["domain"], "Second site domain was not saved")
    require(as_object(owner.request("GET", site_a_url + "/settings")).get("blog_enabled") is False, "Blog module must be disabled by default")
    require(as_object(owner.request("GET", site_b_url + "/settings")).get("blog_enabled") is False, "Second site's blog must be disabled by default")
    for path in [site_a_url, site_a_url + "/contents", site_a_url + "/settings", site_a_url + "/dashboard"]:
        outsider.request("GET", path, status=401)
    ok("multiple client sites have separate identities and protected administration")

    starter_template_checks(owner, outsider, client_base, site_a_url, site_a_home, "/s/" + site_b_payload["slug"], ok)
    site_structure_checks(owner, outsider, site_a_url, site_b_url, site_a_home, ok)
    extranet_checks(owner, client_base, initial_sites[0], [site_a, site_b], nonce, ok)
    visitor_account_checks(owner, client_base, initial_sites[0], site_b, nonce, ok)

    initial_dashboard = as_object(owner.request("GET", site_a_url + "/dashboard"))
    require(all(initial_dashboard.get(key) == 0 for key in ["pages", "articles", "drafts", "published"]), "Disposable CMS contains unexpected content")
    ok("dashboard uses initially empty content")

    page = {
        "kind": "page", "title": "Original page <script>integration-title</script>", "slug": f"test-page-{nonce}",
        "excerpt": 'Original summary & "quoted"', "body": "Original content\nSecond paragraph.\n\n<script>integration-body</script>",
    }
    owner.request("POST", site_a_url + "/contents", page, origin=None, status=403)
    owner.request("POST", site_a_url + "/contents", page, origin="https://attacker.invalid", status=403)
    owner.request("POST", site_a_url + "/contents", page, content_type="text/plain", status=415)
    require(as_array(owner.request("GET", site_a_url + "/contents")) == [], "Rejected mutations must not create content")
    ok("authenticated mutations require same-origin JSON")

    owner.request("POST", site_a_url + "/contents", {**page, "slug": "../admin"}, status=422)
    owner.request("POST", site_a_url + "/contents", {**page, "slug": "admin"}, status=422)
    ok("invalid and reserved slugs are rejected")

    created_page = as_object(owner.request("POST", site_a_url + "/contents", page, status=201))
    page_id = created_page.get("id")
    require(bool(page_id), "Content id missing")
    page_url = site_a_url + "/contents/" + urllib.parse.quote(str(page_id), safe="")
    require(created_page.get("published") is False, "New content must be a draft")
    require(created_page.get("kind") == "page", "Created content kind differs")
    require(any(row.get("id") == page_id for row in as_array(owner.request("GET", site_a_url + "/contents?kind=page"))), "Page missing from page list")
    require(not any(row.get("id") == page_id for row in as_array(owner.request("GET", site_a_url + "/contents?kind=article"))), "Page leaked into article list")
    require(as_object(owner.request("GET", page_url)).get("body") == page["body"], "Draft body was not persisted")
    owner.request("POST", site_a_url + "/contents", page, status=409)
    ok("draft creation, persistence, kind filters and unique slug")

    require(not any(row.get("slug") == page["slug"] for row in as_array(outsider.request("GET", public_a + "/contents"))), "Draft leaked into public listing")
    public_old_url = public_a + "/contents/" + page["slug"]
    outsider.request("GET", public_old_url, status=404)
    html_old_url = site_a_home + "/pages/" + page["slug"]
    outsider.request_html(html_old_url, status=404)
    require(html.escape(page["title"]) not in outsider.request_html(site_a_home), "Draft leaked into the server-rendered site homepage")
    ok("unpublished drafts are inaccessible to visitors")

    published = as_object(owner.request("POST", page_url + "/publish"))
    require(published.get("published") is True, "Publish did not activate content")
    live = as_object(outsider.request("GET", public_old_url))
    require(live.get("body") == page["body"] and live.get("title") == page["title"], "Publication snapshot differs from draft")
    ok("explicit publication exposes a snapshot")
    rendered = outsider.request_html(html_old_url)
    assert_public_metadata(rendered, page["title"], page["excerpt"], origin + html_old_url)
    require(html.escape(page["title"]) in rendered and "Original content" in rendered, "Published content must be present in HTML before JavaScript runs")
    require("&lt;script&gt;integration-body&lt;/script&gt;" in rendered, "Published body markup was not escaped")
    require("<script>integration-title</script>" not in rendered and "<script>integration-body</script>" not in rendered, "Published text was interpreted as executable markup")
    outsider.request_html(site_a_home + "/articles/" + page["slug"], status=404)
    require(html.escape(page["title"]) in outsider.request_html(site_a_home), "Published page missing from server-rendered homepage")
    ok("server-rendered pages contain escaped publication text and correct metadata")

    page_b = {**page, "title": "Beta page", "body": "Beta independent body"}
    created_page_b = as_object(owner.request("POST", site_b_url + "/contents", page_b, status=201))
    page_b_id = created_page_b.get("id")
    require(bool(page_b_id), "Second site content id missing")
    page_b_url = site_b_url + "/contents/" + urllib.parse.quote(str(page_b_id), safe="")
    owner.request("POST", page_b_url + "/publish")
    public_b_page = public_b + "/contents/" + page_b["slug"]
    require(as_object(outsider.request("GET", public_b_page)).get("body") == page_b["body"], "Second site publication differs")
    require(as_object(outsider.request("GET", public_old_url)).get("body") == page["body"], "Second site overwrote first site publication")
    require(len(as_array(owner.request("GET", site_b_url + "/contents"))) == 1, "Second site list includes foreign content")
    foreign_page_url = site_b_url + "/contents/" + urllib.parse.quote(str(page_id), safe="")
    owner.request("GET", foreign_page_url, status=404)
    owner.request("PUT", foreign_page_url, {key: value for key, value in page_b.items() if key != "kind"}, status=404)
    owner.request("POST", foreign_page_url + "/publish", status=404)
    owner.request("POST", foreign_page_url + "/unpublish", status=404)
    owner.request("DELETE", foreign_page_url, status=404)
    require(as_object(owner.request("GET", page_url)).get("body") == page["body"], "Wrong-site mutation changed the original draft")
    require(as_object(outsider.request("GET", public_old_url)).get("body") == page["body"], "Wrong-site mutation changed the original snapshot")
    ok("identical slugs coexist across sites and wrong-site content access is rejected")

    changed_page = {**page, "title": "Updated title", "slug": f"updated-page-{nonce}", "excerpt": "Updated summary", "body": "Updated body"}
    saved = as_object(owner.request("PUT", page_url, {key: value for key, value in changed_page.items() if key != "kind"}))
    require(saved.get("has_unpublished_changes") is True, "Saved changes must be marked unpublished")
    live = as_object(outsider.request("GET", public_old_url))
    require(live.get("body") == page["body"] and live.get("slug") == page["slug"], "Editing a draft changed the live snapshot")
    public_new_url = public_a + "/contents/" + changed_page["slug"]
    outsider.request("GET", public_new_url, status=404)
    html_new_url = site_a_home + "/pages/" + changed_page["slug"]
    outsider.request_html(html_new_url, status=404)
    rendered = outsider.request_html(html_old_url)
    require(html.escape(page["title"]) in rendered and changed_page["title"] not in rendered, "Server rendering exposed an unpublished edit")
    public_listing = as_array(outsider.request("GET", public_a + "/contents"))
    require(any(row.get("slug") == page["slug"] for row in public_listing), "Published slug disappeared before republication")
    require(not any(row.get("slug") == changed_page["slug"] for row in public_listing), "Draft slug leaked into public listing")
    ok("editing body and slug preserves the previous publication")
    owner.request("POST", site_a_url + "/contents", page, status=409)
    owner.request("POST", site_a_url + "/contents", changed_page, status=409)
    ok("both the current draft slug and live snapshot slug remain reserved")

    republished = as_object(owner.request("POST", page_url + "/publish"))
    require(republished.get("has_unpublished_changes") is False, "Republication must clear pending-change state")
    live = as_object(outsider.request("GET", public_new_url))
    require(live.get("body") == changed_page["body"] and live.get("title") == changed_page["title"], "Republication did not update the snapshot")
    outsider.request("GET", public_old_url, status=404)
    outsider.request_html(html_old_url, status=404)
    rendered = outsider.request_html(html_new_url)
    assert_public_metadata(rendered, changed_page["title"], changed_page["excerpt"], origin + html_new_url)
    require(changed_page["body"] in rendered, "Server rendering did not update after republication")
    require(as_object(outsider.request("GET", public_b_page)).get("body") == page_b["body"], "Republication changed a different site")
    ok("republication updates content and canonical slug together")

    article = {"kind": "article", "title": "An article", "slug": f"article-{nonce}", "excerpt": "Article summary", "body": "Article body"}
    owner.request("POST", site_a_url + "/contents", article, status=409)
    settings_a = as_object(owner.request("GET", site_a_url + "/settings"))
    owner.request("PUT", site_a_url + "/settings", {**settings_a, "blog_enabled": True})
    owner.request("POST", site_b_url + "/contents", article, status=409)
    require(as_object(outsider.request("GET", public_b)).get("blog_enabled") is False, "Enabling the first site's blog enabled another client's blog")
    require("Les premières histoires se préparent." in outsider.request_html(site_a_home + "/blog"), "Enabled but empty blog must retain its starter state")
    created_article = as_object(owner.request("POST", site_a_url + "/contents", article, status=201))
    require(article["title"] not in outsider.request_html(site_a_home + "/blog"), "Unpublished article leaked into the blog index")
    article_id = created_article.get("id")
    require(bool(article_id), "Article id missing")
    article_url = site_a_url + "/contents/" + urllib.parse.quote(str(article_id), safe="")
    owner.request("POST", article_url + "/publish")
    require(as_object(outsider.request("GET", public_a + "/contents/" + article["slug"])).get("kind") == "article", "Public article kind differs")
    html_article_url = site_a_home + "/articles/" + article["slug"]
    rendered_article = outsider.request_html(html_article_url)
    assert_public_metadata(rendered_article, article["title"], article["excerpt"], origin + html_article_url)
    require(article["body"] in rendered_article, "Published article must be server-rendered")
    blog_index = outsider.request_html(site_a_home + "/blog")
    require(article["title"] in blog_index and 'href="' + html_article_url + '"' in blog_index, "Blog index is missing its published article link")
    require(article["title"] not in outsider.request_html("/s/" + site_b_payload["slug"] + "/blog"), "Blog index exposed another site's article")
    outsider.request_html(site_a_home + "/pages/" + article["slug"], status=404)
    dashboard = as_object(owner.request("GET", site_a_url + "/dashboard"))
    require(dashboard.get("pages") == 1 and dashboard.get("articles") == 1 and dashboard.get("published") == 2, "Dashboard counts do not match stored content")
    dashboard_b = as_object(owner.request("GET", site_b_url + "/dashboard"))
    require(dashboard_b.get("pages") == 1 and dashboard_b.get("articles") == 0 and dashboard_b.get("published") == 1, "Second site dashboard includes another site's content")
    ok("articles publish and dashboards count each site's own content")

    owner.request("PUT", site_a_url + "/settings", {**settings_a, "blog_enabled": False})
    outsider.request("GET", public_a + "/contents/" + article["slug"], status=404)
    outsider.request_html(html_article_url, status=404)
    require(article["title"] not in outsider.request_html(site_a_home), "Disabled blog leaked into server-rendered homepage")
    disabled_blog = outsider.request_html(site_a_home + "/blog")
    require("Les premières histoires se préparent." in disabled_blog and article["title"] not in disabled_blog and article["excerpt"] not in disabled_blog, "Disabled blog index leaked a published article")
    require(not any(row.get("kind") == "article" for row in as_array(outsider.request("GET", public_a + "/contents"))), "Disabled blog exposes articles in the public list")
    require(as_object(owner.request("GET", article_url)).get("body") == article["body"], "Disabling blog destroyed article data")
    owner.request("PUT", article_url, {**{key: value for key, value in article.items() if key != "kind"}, "title": "Blocked edit"}, status=409)
    owner.request("POST", article_url + "/publish", status=409)
    owner.request("PUT", site_a_url + "/settings", {**settings_a, "blog_enabled": True})
    require(as_object(outsider.request("GET", public_a + "/contents/" + article["slug"])).get("body") == article["body"], "Re-enabling blog did not restore the published article")
    require(article["body"] in outsider.request_html(html_article_url), "Re-enabling blog did not restore the server-rendered article")
    require(article["title"] in outsider.request_html(site_a_home + "/blog"), "Re-enabling blog did not restore its published index")
    require(as_object(outsider.request("GET", public_b)).get("blog_enabled") is False, "Blog module changes affected another site")
    ok("blog activation is isolated and disabling preserves published article data")

    unpublished = as_object(owner.request("POST", page_url + "/unpublish"))
    require(unpublished.get("published") is False, "Unpublish did not deactivate content")
    outsider.request("GET", public_new_url, status=404)
    outsider.request_html(html_new_url, status=404)
    require(as_object(owner.request("GET", page_url)).get("body") == changed_page["body"], "Unpublish lost the editable draft")
    ok("unpublish removes public access and preserves the draft")

    settings = {"site_name": "CMS integration " + nonce, "description": "Description for integration test", "blog_enabled": True}
    settings_b_before = as_object(owner.request("GET", site_b_url + "/settings"))
    owner.request("PUT", site_a_url + "/settings", settings, origin="https://attacker.invalid", status=403)
    owner.request("PUT", site_a_url + "/settings", settings)
    stored_settings = as_object(owner.request("GET", site_a_url + "/settings"))
    public_settings = as_object(outsider.request("GET", public_a))
    require(all(stored_settings.get(key) == value and public_settings.get(key) == value for key, value in settings.items()), "Site settings were not persisted and published")
    require(as_object(owner.request("GET", site_b_url + "/settings")) == settings_b_before, "Changing first site settings changed second site settings")
    settings_b = {"site_name": "Independent beta", "description": "Separate client description", "blog_enabled": False}
    owner.request("PUT", site_b_url + "/settings", settings_b)
    require(all(as_object(outsider.request("GET", public_b)).get(key) == value for key, value in settings_b.items()), "Second site settings were not published independently")
    require(all(as_object(outsider.request("GET", public_a)).get(key) == value for key, value in settings.items()), "Second site settings overwrote first site's public settings")
    ok("each client's settings persist and publish independently")

    owner.request("DELETE", article_url, status=204)
    owner.request("GET", article_url, status=404)
    outsider.request("GET", public_a + "/contents/" + article["slug"], status=404)
    owner.request("DELETE", page_url, status=204)
    require(as_array(owner.request("GET", site_a_url + "/contents")) == [], "Deleted content remains in administration")
    require(as_array(outsider.request("GET", public_a + "/contents")) == [], "Deleted content remains publicly accessible")
    require(as_object(outsider.request("GET", public_b_page)).get("body") == page_b["body"], "Deletion removed another client's publication")
    owner.request("DELETE", page_b_url, status=204)
    outsider.request("GET", public_b_page, status=404)
    ok("deletion removes only the selected client's draft and public snapshot")

    builder_checks(owner, outsider, site_a_url, site_a_home, site_b_url, "/s/" + site_b["slug"], nonce, ok)
    builder_widget_checks(owner, outsider, site_a_url, site_a_home, "/s/" + site_b["slug"], nonce, ok)
    previous_cookie = owner.cookie
    require(bool(previous_cookie), "Missing session before revocation test")
    owner.request("POST", "/api/auth/logout", status=204)
    stale_session = Client(base, origin)
    stale_session.cookie = previous_cookie
    stale_session.request("GET", "/api/auth/me", status=401)
    stale_session.request("PUT", site_a_url + "/settings", settings, status=401)
    ok("logout revokes the server-side session, including cookie replay")

    print(f"SUCCESS {passed} integration checks passed", flush=True)


if __name__ == "__main__":
    try:
        main()
    except TestFailure as failure:
        print(f"FAIL {failure}", file=sys.stderr)
        sys.exit(1)
    except KeyboardInterrupt:
        print("FAIL interrupted", file=sys.stderr)
        sys.exit(130)
    except Exception as failure:
        # Never leak a secret through an unexpected exception representation.
        print(f"FAIL unexpected {type(failure).__name__}; inspect the test implementation", file=sys.stderr)
        sys.exit(1)
