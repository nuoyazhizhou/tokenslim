# Batch process all test files
$testFiles = @(
    "android_build_failure.txt",
    "android_build_success.txt",
    "gcc_build_failure-1.txt",
    "gcc_build_failure-2.txt",
    "gcc_build_failure-3.txt",
    "gcc_build_failure-4.txt",
    "gcc_build_success.txt",
    "gcc_build_utf8.txt",
    "gcc_coverity_success-1.txt",
    "gcc_coverity_success-2.txt",
    "ios_build_failure.txt",
    "ios_build_success.txt",
    "jenkins_build_failure.txt",
    "maven_java_build_failure.txt",
    "maven_java_build_success.txt",
    "nodejs_build_failure.txt"
)

$exePath = ".\target\release\tokenslim.exe"
$inputDir = ".\tests\data"
$outputDir = ".\tests\output"

foreach ($file in $testFiles) {
    $inputPath = Join-Path $inputDir $file
    $outputFile = [System.IO.Path]::GetFileNameWithoutExtension($file) + ".json"
    $outputPath = Join-Path $outputDir $outputFile

    Write-Host "Processing: $file -> $outputFile"
    & $exePath -i $inputPath -o $outputPath 2>&1 | Out-Null
}

Write-Host "All files processed!"
