#!/usr/bin/env python3
"""Tests for install.sh. Docker is a local fake that records its arguments;
nothing is built or started. Must run as root (the installer sets owners)."""
import os
import re
import shutil
import stat
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
FAKE_DOCKER = """#!/bin/sh
echo "$*" >> "$DOCKER_LOG"
if [ "$1 $2" = "compose version" ]; then echo "Docker Compose version v2.40.0"; fi
exit 0
"""


class Install(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp())
        self.repo = self.tmp / "awningcms"
        self.repo.mkdir()
        shutil.copy2(ROOT / "install.sh", self.repo / "install.sh")
        shutil.copytree(ROOT / "infra", self.repo / "infra")
        self.bin = self.tmp / "bin"
        self.bin.mkdir()
        docker = self.bin / "docker"
        docker.write_text(FAKE_DOCKER)
        docker.chmod(0o755)
        self.log = self.tmp / "docker.log"

    def tearDown(self):
        shutil.rmtree(self.tmp)

    def run_install(self, **answers):
        env = {
            "PATH": f"{self.bin}:/usr/bin:/bin",
            "DOCKER_LOG": str(self.log),
            "AWNING_YES": "1",
            "AWNING_ADMIN_HOST": "cms.example.com",
            "AWNING_SITE_HOST": "www.example.com",
            "AWNING_PUBLIC_IP": "203.0.113.7",
        }
        env.update(answers)
        return subprocess.run(["bash", str(self.repo / "install.sh")], cwd=self.repo,
                              env=env, capture_output=True, text=True)

    def settings(self):
        lines = (self.repo / "infra/.env").read_text().splitlines()
        return dict(line.split("=", 1) for line in lines if "=" in line and not line.startswith("#"))

    def secrets(self):
        return {p.name: p.read_bytes() for p in (self.repo / "private").iterdir()}

    def test_first_run_creates_private_secrets(self):
        result = self.run_install()
        self.assertEqual(result.returncode, 0, result.stderr)
        private = self.repo / "private"
        self.assertEqual(stat.S_IMODE(private.stat().st_mode), 0o700)
        expected = {"db-password.secret": (0o400, 70, 70),
                    "app-db-password.secret": (0o440, 10001, 70),
                    "setup-token.secret": (0o400, 10001, 10001)}
        for name, (mode, uid, gid) in expected.items():
            info = (private / name).stat()
            self.assertEqual((stat.S_IMODE(info.st_mode), info.st_uid, info.st_gid), (mode, uid, gid), name)
            self.assertRegex((private / name).read_text(), r"^[0-9a-f]{64}\n?$")
        settings = self.settings()
        self.assertEqual(settings["ADMIN_HOST"], "cms.example.com")
        self.assertEqual(settings["SITE_HOST"], "www.example.com")
        self.assertEqual(settings["PUBLIC_ORIGIN"], "https://cms.example.com")
        self.assertEqual(settings["PRIMARY_SITE_ORIGIN"], "https://www.example.com")
        self.assertEqual(settings["PUBLIC_IPS"], "203.0.113.7")
        self.assertEqual(settings["CADDYFILE"], "Caddyfile")
        calls = self.log.read_text()
        self.assertIn("build -t awningcms:", calls)
        self.assertRegex(calls, r"compose --env-file \S*infra/\.env -f \S*infra/compose\.yaml up -d --wait")

    def test_rerun_keeps_existing_secrets(self):
        self.assertEqual(self.run_install().returncode, 0)
        before = self.secrets()
        result = self.run_install()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.secrets(), before)

    def test_never_prints_the_setup_token(self):
        result = self.run_install()
        token = (self.repo / "private/setup-token.secret").read_text().strip()
        self.assertNotIn(token, result.stdout + result.stderr)
        self.assertIn("private/setup-token.secret", result.stdout)
        self.assertIn("https://cms.example.com/admin/setup", result.stdout)

    def test_refuses_without_docker_compose(self):
        # Docker without its Compose plugin: "docker compose version" fails.
        (self.bin / "docker").write_text("#!/bin/sh\necho \"$*\" >> \"$DOCKER_LOG\"\nexit 1\n")
        result = self.run_install()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Docker", result.stderr)
        self.assertFalse((self.repo / "private").exists())

    def test_one_host_uses_the_one_host_caddyfile(self):
        result = self.run_install(AWNING_SITE_HOST="cms.example.com")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.settings()["CADDYFILE"], "Caddyfile.one-host")

    def test_custom_https_port_is_part_of_the_origins(self):
        result = self.run_install(AWNING_HTTP_PORT="18081", AWNING_HTTPS_PORT="18444")
        self.assertEqual(result.returncode, 0, result.stderr)
        settings = self.settings()
        self.assertEqual(settings["PUBLIC_ORIGIN"], "https://cms.example.com:18444")
        self.assertEqual(settings["PRIMARY_SITE_ORIGIN"], "https://www.example.com:18444")
        self.assertEqual((settings["HTTP_PORT"], settings["HTTPS_PORT"]), ("18081", "18444"))

    def test_uses_the_override_file_when_present(self):
        (self.repo / "infra/compose.override.yaml").write_text("services: {}\n")
        result = self.run_install()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertRegex(self.log.read_text(), r"-f \S*infra/compose\.yaml -f \S*infra/compose\.override\.yaml up -d --wait")

    def test_rejects_a_missing_or_malformed_ip(self):
        for value in ["203.0.113.7; rm -rf /", "pas-une-ip"]:
            result = self.run_install(AWNING_PUBLIC_IP=value)
            self.assertNotEqual(result.returncode, 0, value)
        self.assertFalse((self.repo / "private").exists())

    def test_rejects_an_invalid_host(self):
        result = self.run_install(AWNING_ADMIN_HOST="exa mple.com")
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse((self.repo / "private").exists())


if __name__ == "__main__":
    unittest.main()
