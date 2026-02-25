Param(
    [string]$ModelPath = "MiniMax-M2.1.q2_k.gguf",
    [string]$PromptsPath = "data/prompts.txt",
    [string]$OutDir = "outputs\\local_run",
    [int]$Threads = 8,
    [int]$Ctx = 512,
    [int]$Batch = 256,
    [int]$Dim = 256,
    [int]$Steps = 500
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $MyInvocation.MyCommand.Path
cd $root

function Log($msg) { $ts = Get-Date -Format "yyyy-MM-dd HH:mm:ss"; "$ts $msg" | Tee-Object -FilePath "$OutDir\run.log" -Append }

if (-not (Test-Path $OutDir)) { New-Item -ItemType Directory -Path $OutDir | Out-Null }

Log "Building teacher (Windows/MSVC)..."
powershell -File teacher\build_teacher.ps1

Log "Running teacher to extract logits/tokens..."
.\teacher\serve_teacher.exe `
    --model $ModelPath `
    --prompts $PromptsPath `
    --output "$OutDir\logits.bin" `
    --tokens "$OutDir\tokens.bin" `
    --ctx $Ctx --threads $Threads --batch $Batch

Log "Training student..."
python student\train_student.py `
    --logits "$OutDir\logits.bin" `
    --tokens "$OutDir\tokens.bin" `
    --output "$OutDir\student.pt" `
    --dim $Dim --steps $Steps --vocab 32000

Log "Validating..."
python agent\simple_validate.py `
    --logits "$OutDir\logits.bin" `
    --tokens "$OutDir\tokens.bin" `
    --student "$OutDir\student.pt" `
    --dim $Dim --vocab 32000 --threshold 0.85 | Tee-Object -FilePath "$OutDir\run.log" -Append

Log "Done. Outputs in $OutDir"
