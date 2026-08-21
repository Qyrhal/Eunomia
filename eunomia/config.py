import os
from pathlib import Path

VAULT_PATH = Path(os.environ.get("EUNOMIA_VAULT_PATH", "./vault")).expanduser()
DB_PATH = os.environ.get("EUNOMIA_DB_PATH", "eunomia.db")
