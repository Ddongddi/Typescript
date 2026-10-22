# After build.ps1, exercise the shipped JNI library on a running emulator/device.
# Does not install or select an input method. Example: ./smoke.ps1 -Serial emulator-5554
param([Parameter(Mandatory = $true)][string] $Serial)
$ErrorActionPreference = 'Stop'
$sdk = if ($env:ANDROID_HOME) { $env:ANDROID_HOME } else { Join-Path $env:LOCALAPPDATA 'Android/Sdk' }
$exe = if ($env:OS -eq 'Windows_NT') { '.exe' } else { '' }
$bat = if ($env:OS -eq 'Windows_NT') { '.bat' } else { '' }
$adb = Join-Path $sdk "platform-tools/adb$exe"
$tools = Get-ChildItem (Join-Path $sdk 'build-tools') | Sort-Object { [version] $_.Name } | Select-Object -Last 1
$device = @('-s', $Serial)
$build = Join-Path $PSScriptRoot 'build/smoke'
$classes = Join-Path $build 'classes'
$dex = Join-Path $build 'dex'
$remote = '/data/local/tmp/tacit-jni-smoke'
function Run($what, [scriptblock] $block) {
    & $block
    if ($LASTEXITCODE -ne 0) { throw "$what failed ($LASTEXITCODE)" }
}
$abi = (Run 'device ABI' { & $adb @device shell getprop ro.product.cpu.abi }).Trim()
if ($abi -notin @('arm64-v8a', 'x86_64')) { throw "unsupported ABI: $abi" }
$library = Join-Path $PSScriptRoot "build/jniLibs/$abi/libtacit_android.so"
if (-not (Test-Path -LiteralPath $library)) { throw 'run build.ps1 first' }
New-Item -ItemType Directory -Force $classes, $dex | Out-Null
$androidJar = Join-Path $sdk 'platforms/android-34/android.jar'
$sources = @(Join-Path $PSScriptRoot 'app/src'; Join-Path $PSScriptRoot 'build/gen'; Join-Path $PSScriptRoot 'tests') |
    ForEach-Object { Get-ChildItem $_ -Recurse -Filter *.java } | ForEach-Object FullName
Run 'javac' { javac --release 8 -encoding UTF-8 -classpath $androidJar -d $classes @sources }
$classFiles = Get-ChildItem $classes -Recurse -Filter *.class | ForEach-Object FullName
Run 'd8' {
    & "$($tools.FullName)/d8$bat" --min-api 26 --lib $androidJar --output $dex @classFiles
}
$jar = Join-Path $build 'smoke.jar'
Run 'jar' { jar cf $jar -C $dex classes.dex }
Run 'mkdir' { & $adb @device shell mkdir -p $remote }
Run 'push library' { & $adb @device push $library "$remote/" }
Run 'push test' { & $adb @device push $jar "$remote/" }
# The reading-choice check learns; discard only this harness's state before each run.
Run 'clear smoke learning' { & $adb @device shell rm -f "$remote/lexicon.json" }
Run 'JNI smoke' {
    & $adb @device shell "CLASSPATH=$remote/smoke.jar app_process -Djava.library.path=$remote /system/bin io.github.iamallyouneed.tacit.Smoke $remote"
}
Run 'cursor smoke' {
    & $adb @device shell "CLASSPATH=$remote/smoke.jar app_process -Djava.library.path=$remote /system/bin io.github.iamallyouneed.tacit.CursorSmoke $remote"
}
