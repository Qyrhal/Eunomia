from django.core.management.base import BaseCommand

from tasks.demo_seed import clear_demo_data, seed_demo_data


class Command(BaseCommand):
    help = "Populate (or clear) realistic fake tasks/projects for trying the app out."

    def add_arguments(self, parser):
        parser.add_argument("--clear", action="store_true", help="Remove demo data instead of creating it")
        parser.add_argument("--seed", type=int, default=None, help="Random seed, for reproducible demo data")

    def handle(self, *args, **options):
        if options["clear"]:
            result = clear_demo_data()
            self.stdout.write(self.style.SUCCESS(f"Removed {result['projects_removed']} demo project(s)."))
            return
        result = seed_demo_data(seed=options["seed"])
        self.stdout.write(
            self.style.SUCCESS(f"Seeded {result['projects']} demo project(s) with {result['tasks']} tasks.")
        )
