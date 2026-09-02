"""Long-lived sync worker. Run alongside the web server:

    python manage.py run_worker

Polls each enabled source on its interval, backfills embeddings, and runs any
trigger schedules/crons (#39). One process; a systemd unit / compose service.
"""

from django.core.management.base import BaseCommand


class Command(BaseCommand):
    help = "Run the Eunomia sync + trigger worker (APScheduler)."

    def handle(self, *args, **opts):
        from sources.scheduler import build_scheduler

        sched = build_scheduler()
        self.stdout.write(self.style.SUCCESS(f"worker up — {len(sched.get_jobs())} jobs"))
        try:
            sched.start()
        except (KeyboardInterrupt, SystemExit):
            self.stdout.write("worker stopped")
