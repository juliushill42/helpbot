import torch
import torch.nn.functional as F


class QualityValidator:
    def __init__(self, threshold: float = 0.85):
        self.threshold = threshold

    def validate_distillation(self, teacher_logits: torch.Tensor, student_logits: torch.Tensor):
        """
        Calculates KL Divergence to measure how well the student
        matches the teacher's 'soft probabilities'.
        Returns (is_pass, similarity_score).
        """
        T = 2.0
        p = F.log_softmax(student_logits / T, dim=-1)
        q = F.softmax(teacher_logits / T, dim=-1)
        divergence = F.kl_div(p, q, reduction="batchmean") * (T**2)
        similarity = 1.0 / (1.0 + divergence.item())
        return similarity >= self.threshold, similarity
