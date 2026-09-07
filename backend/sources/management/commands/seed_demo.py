"""Seed realistic demo data (bank / calendar / email / recordings / tasks) and
push it into the cache via the demo source. `--clear` reverses it."""

from django.core.management.base import BaseCommand


class Command(BaseCommand):
    help = "Seed or clear Eunomia demo data (cache + tasks)."

    def add_arguments(self, parser):
        parser.add_argument("--clear", action="store_true")
        parser.add_argument("--seed", type=int, default=None)

    def handle(self, *args, **opts):
        from connectors.demo_seed import (
            clear_demo_bank_data, clear_demo_pocket_data,
            seed_demo_bank_data, seed_demo_pocket_data,
        )
        from connectors.models import Connector
        from tasks.demo_seed import clear_demo_data, seed_demo_data

        if opts["clear"]:
            clear_demo_bank_data(); clear_demo_pocket_data()
            clear_demo_data()
            Connector.objects.filter(kind="demo").delete()
            from cache.models import CacheRecord
            n, _ = CacheRecord.objects.filter(source="demo").delete()
            self.stdout.write(self.style.SUCCESS(f"Cleared demo data ({n} cache rows)."))
            return

        s = opts["seed"]
        seed_demo_bank_data(seed=s); seed_demo_pocket_data(seed=s)
        seed_demo_data(seed=s)
        Connector.objects.update_or_create(kind="demo", defaults={"enabled": True})

        from sources import registry
        registry.discover()
        report, _ = registry.run_sync("demo")
        self.stdout.write(self.style.SUCCESS(f"Seeded + synced demo data: {report.as_dict()}"))
