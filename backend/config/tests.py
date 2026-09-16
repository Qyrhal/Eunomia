from django.test import TestCase, override_settings

from config.auth import token_ok


class TokenOkTests(TestCase):
    def test_open_when_no_token_configured(self):
        with override_settings(EUNOMIA_API_TOKEN=""):
            self.assertTrue(token_ok(None))
            self.assertTrue(token_ok("Bearer anything"))

    @override_settings(EUNOMIA_API_TOKEN="s3cr3t")
    def test_requires_matching_bearer_when_configured(self):
        self.assertFalse(token_ok(None))
        self.assertFalse(token_ok("s3cr3t"))
        self.assertFalse(token_ok("Bearer wrong"))
        self.assertFalse(token_ok("Basic s3cr3t"))
        self.assertTrue(token_ok("Bearer s3cr3t"))
        self.assertTrue(token_ok("bearer s3cr3t"))


class ApiPermissionTests(TestCase):
    @override_settings(EUNOMIA_API_TOKEN="s3cr3t")
    def test_api_route_rejected_without_token(self):
        self.assertEqual(self.client.get("/api/tasks/").status_code, 403)

    @override_settings(EUNOMIA_API_TOKEN="s3cr3t")
    def test_api_route_ok_with_token(self):
        resp = self.client.get("/api/tasks/", HTTP_AUTHORIZATION="Bearer s3cr3t")
        self.assertEqual(resp.status_code, 200)

    def test_api_route_open_when_unset(self):
        with override_settings(EUNOMIA_API_TOKEN=""):
            self.assertEqual(self.client.get("/api/tasks/").status_code, 200)
