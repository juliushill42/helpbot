import asyncio
import json
import logging
import struct
from dataclasses import dataclass
from pathlib import Path
from typing import List, Optional

import yaml
from notifier import send_webhook_alert
from resource_allocator import compute_max_parallel
from validator import QualityValidator


@dataclass
class DistillationJob:
    model_name: str
    model_path: str
    output_dir: Path
    status: str = "pending"
    progress: float = 0.0
    error: Optional[str] = None
    similarity: Optional[float] = None


class AutonomousDistiller:
    def __init__(self, config_path: str = "agent/config.yaml"):
        self.root = Path(__file__).resolve().parent.parent
        with open(self.root / config_path) as f:
            self.config = yaml.safe_load(f)

        self.job_queue: List[DistillationJob] = []
        self.processed: List[DistillationJob] = []
        self.active_workers = 0
        self.max_parallel = compute_max_parallel(
            self.config.get("max_parallel_jobs", 1),
            self.config.get("max_ram_gb", 16),
            self.config.get("per_job_ram_gb", 8),
        )

        log_dir = self.root / "agent"
        log_dir.mkdir(parents=True, exist_ok=True)
        logging.basicConfig(
            level=logging.INFO,
            format="%(asctime)s [%(levelname)s] %(message)s",
            handlers=[
                logging.FileHandler(log_dir / "distiller.log"),
                logging.StreamHandler(),
            ],
        )
        self.logger = logging.getLogger(__name__)
        self.validator = QualityValidator(threshold=self.config.get("validation_threshold", 0.85))
        self.webhook_url = self.config.get("webhook_url")
        self.webhook_secret = self.config.get("webhook_secret")
        self._write_state()

    def add_models_from_directory(self, model_dir: str):
        model_path = Path(model_dir)
        for gguf_file in model_path.glob("*.gguf"):
            job = DistillationJob(
                model_name=gguf_file.stem,
                model_path=str(gguf_file),
                output_dir=(self.root / self.config.get("output_base_dir", "outputs") / gguf_file.stem),
            )
            self.job_queue.append(job)
            self.logger.info(f"Queued: {job.model_name}")

    def add_models_from_config(self, config_file: str):
        with open(config_file) as f:
            models = yaml.safe_load(f)["models"]
        for model in models:
            job = DistillationJob(
                model_name=model["name"],
                model_path=model["path"],
                output_dir=(self.root / model.get("output_dir", f"outputs/{model['name']}")),
            )
            self.job_queue.append(job)
            self.logger.info(f"Queued: {job.model_name}")

    async def process_job(self, job: DistillationJob):
        try:
            job.status = "running"
            self.logger.info(f"Starting: {job.model_name}")
            job.output_dir.mkdir(parents=True, exist_ok=True)

            job.progress = 0.1
            await self._run_teacher_extraction(job)
            self._write_state()

            job.progress = 0.5
            await self._run_student_training(job)
            self._write_state()

            job.progress = 0.7
            similarity = await self._validate_job(job)
            job.similarity = similarity
            job.progress = 0.85

            if similarity is None or similarity < self.validator.threshold:
                raise RuntimeError(f"Quality too low: {similarity}")

            job.progress = 0.9
            await self._export_coreml(job)
            self._write_state()

            # Hook: send notification
            try:
                if self.webhook_url and self.webhook_secret:
                    await send_webhook_alert("distillation_complete", job.model_name, job.status, secret=self.webhook_secret, url=self.webhook_url)
            except Exception as e:  # noqa: BLE001
                self.logger.warning(f"Webhook send failed: {e}")

            job.status = "complete"
            job.progress = 1.0
            self.logger.info(f"Complete: {job.model_name}")

        except Exception as e:  # noqa: BLE001
            job.status = "failed"
            job.error = str(e)
            self.logger.error(f"Failed: {job.model_name} - {e}")
            try:
                if self.webhook_url and self.webhook_secret:
                    await send_webhook_alert("distillation_failed", job.model_name, job.status, secret=self.webhook_secret, url=self.webhook_url)
            except Exception as hook_err:  # noqa: BLE001
                self.logger.warning(f"Webhook send failed: {hook_err}")
        finally:
            self.processed.append(job)
            self._write_state()

    async def _run_teacher_extraction(self, job: DistillationJob):
        cmd = [
            str(self.root / "teacher" / "serve_teacher"),
            "--model",
            job.model_path,
            "--output",
            str(job.output_dir / "logits.bin"),
            "--tokens",
            str(job.output_dir / "tokens.bin"),
            "--prompts",
            str(self.root / self.config["dataset_path"]),
        ]
        process = await asyncio.create_subprocess_exec(
            *cmd, stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.PIPE
        )
        stdout, stderr = await process.communicate()
        if process.returncode != 0:
            raise RuntimeError(f"Teacher extraction failed: {stderr.decode()}")
        self.logger.info(f"Logits extracted for {job.model_name} ({stdout.decode().strip()})")

    async def _run_student_training(self, job: DistillationJob):
        cmd = [
            "python3",
            str(self.root / "student" / "train_student.py"),
            "--logits",
            str(job.output_dir / "logits.bin"),
            "--tokens",
            str(job.output_dir / "tokens.bin"),
            "--output",
            str(job.output_dir / "student.pt"),
            "--dim",
            str(self.config["student_dim"]),
            "--lr",
            str(self.config["learning_rate"]),
            "--steps",
            str(self.config["training_steps"]),
            "--vocab",
            str(self.config.get("vocab_size", 32000)),
        ]
        process = await asyncio.create_subprocess_exec(
            *cmd, stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.PIPE
        )
        stdout, stderr = await process.communicate()
        if process.returncode != 0:
            raise RuntimeError(f"Training failed: {stderr.decode()}")
        self.logger.info(f"Student trained for {job.model_name} ({stdout.decode().strip()})")

    async def _export_coreml(self, job: DistillationJob):
        cmd = [
            "python3",
            str(self.root / "student" / "export_coreml.py"),
            "--model",
            str(job.output_dir / "student.pt"),
            "--output",
            str(job.output_dir / f"{job.model_name}.mlpackage"),
            "--vocab",
            str(self.config.get("vocab_size", 32000)),
            "--dim",
            str(self.config["student_dim"]),
        ]
        process = await asyncio.create_subprocess_exec(
            *cmd, stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.PIPE
        )
        stdout, stderr = await process.communicate()
        if process.returncode != 0:
            raise RuntimeError(f"Export failed: {stderr.decode()}")
        self.logger.info(f"CoreML exported for {job.model_name} ({stdout.decode().strip()})")

    async def _validate_job(self, job: DistillationJob):
        """
        Lightweight validation: compare a small slice of teacher/student logits.
        """
        try:
            import torch  # type: ignore
        except ImportError:
            self.logger.warning("Validation skipped: torch not installed")
            return None

        teacher_path = job.output_dir / "logits.bin"
        tokens_path = job.output_dir / "tokens.bin"
        student_path = job.output_dir / "student.pt"
        if not (teacher_path.exists() and tokens_path.exists() and student_path.exists()):
            self.logger.warning("Validation skipped: missing artifacts")
            return None

        vocab = self.config.get("vocab_size", 32000)
        sample_tokens = self.config.get("validation_tokens", 4)

        # Read tokens
        tokens = []
        with open(tokens_path, "rb") as tf:
            for _ in range(sample_tokens):
                b = tf.read(4)
                if not b:
                    break
                tokens.append(struct.unpack("i", b)[0])
        if not tokens:
            self.logger.warning("Validation skipped: no tokens available")
            return None
        sample_tokens = len(tokens)

        # Read corresponding logits
        with open(teacher_path, "rb") as lf:
            buf = lf.read(4 * vocab * sample_tokens)
            if len(buf) < 4 * vocab:
                self.logger.warning("Validation skipped: insufficient logits")
                return None
            teacher_logits = torch.tensor(struct.unpack(f"{vocab * sample_tokens}f", buf)).view(sample_tokens, vocab)

        from student.liquid_ssm import LiquidSSM  # type: ignore
        model = LiquidSSM(vocab, d=self.config.get("student_dim", 256))
        model.load_state_dict(torch.load(student_path, map_location="cpu"))
        model.eval()

        state = torch.zeros(1, self.config.get("student_dim", 256))
        student_logits_list = []
        with torch.no_grad():
            for tok in tokens:
                x = torch.tensor([[tok]])
                student_logits, state = model(x, state)
                student_logits_list.append(student_logits.view(-1))
                state = state.detach()
        student_logits = torch.stack(student_logits_list)

        ok, score = self.validator.validate_distillation(teacher_logits.mean(dim=0), student_logits.mean(dim=0))
        self.logger.info(f"Validation similarity={score:.4f} (pass={ok})")
        return score

    async def worker(self):
        while self.job_queue:
            job = self.job_queue.pop(0)
            self.active_workers += 1
            self._write_state()
            await self.process_job(job)
            self.active_workers -= 1
            self._write_state()

    async def run(self):
        self.logger.info(f"Agent starting with {len(self.job_queue)} jobs")
        self.logger.info(f"Max parallel (adaptive): {self.max_parallel}")
        workers = [
            asyncio.create_task(self.worker())
            for _ in range(min(self.max_parallel, len(self.job_queue)))
        ]
        await asyncio.gather(*workers)
        self._generate_report()
        self.logger.info("All jobs complete!")

    def _generate_report(self):
        report = {
            "completed": sum(1 for j in self.processed if j.status == "complete"),
            "failed": sum(1 for j in self.processed if j.status == "failed"),
            "jobs": [
                {
                    "name": j.model_name,
                    "status": j.status,
                    "output": str(j.output_dir) if j.status == "complete" else None,
                    "error": j.error,
                    "similarity": j.similarity,
                }
                for j in self.processed
            ],
        }
        report_path = self.root / "agent" / "report.json"
        with open(report_path, "w") as f:
            json.dump(report, f, indent=2)
        self.logger.info(f"Report saved: {report_path}")
        self._write_state()

    def _write_state(self):
        state = {
            "active_workers": self.active_workers,
            "max_parallel": self.max_parallel,
            "queue_length": len(self.job_queue),
            "processed": [
                {
                    "name": j.model_name,
                    "status": j.status,
                    "progress": j.progress,
                    "similarity": j.similarity,
                    "error": j.error,
                }
                for j in self.processed
            ],
        }
        state_path = self.root / "agent" / "state.json"
        try:
            state_path.write_text(json.dumps(state, indent=2))
        except Exception as e:  # noqa: BLE001
            self.logger.warning(f"Failed to write state: {e}")


if __name__ == "__main__":
    import sys

    agent = AutonomousDistiller()
    if len(sys.argv) > 1:
        if sys.argv[1] == "--scan":
            agent.add_models_from_directory(sys.argv[2])
        elif sys.argv[1] == "--config":
            agent.add_models_from_config(sys.argv[2])
    else:
        agent.add_models_from_directory("./models")

    asyncio.run(agent.run())
