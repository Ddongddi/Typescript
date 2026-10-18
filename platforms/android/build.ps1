# Builds platforms/android/build/tacit.apk without Gradle:
# cargo-ndk (Rust) -> aapt2 -> javac -> d8 -> zip -> zipalign -> apksigner (debug key).
#   .\build.ps1                      arm64 + x86_64 (emulator)
#   .\build.ps1 -Abi arm64-v8a       phones only (smaller)
param([string[]] $Abi = @('arm64-v8a', 'x86_64'), [int] $MinSdk = 26, [int] $TargetSdk = 34)

$ErrorActionPreference = 'Stop'
$here = $PSScriptRoot
$build = Join-Path $here 'build'
$app = Join-Path $here 'app'

$sdk = if ($env:ANDROID_HOME) { $env:ANDROID_HOME } else { Join-Path $env:LOCALAPPDATA 'Android\Sdk' }
$tools = Get-ChildItem (Join-Path $sdk 'build-tools') | Sort-Object { [version] $_.Name } | Select-Object -Last 1
$platform = Get-ChildItem (Join-Path $sdk 'platforms') | Where-Object { $_.Name -eq "android-$TargetSdk" }
if (-not $platform) { throw "platform android-$TargetSdk is not installed in $sdk" }
$androidJar = Join-Path $platform.FullName 'android.jar'
$ndk = if ($env:ANDROID_NDK_HOME) { $env:ANDROID_NDK_HOME } else {
    (Get-ChildItem (Join-Path $sdk 'ndk') | Sort-Object { [version] $_.Name } | Select-Object -Last 1).FullName
}
$env:ANDROID_HOME = $sdk
$env:ANDROID_NDK_HOME = $ndk

function Run($what, [scriptblock] $block) {
    & $block
    if ($LASTEXITCODE -ne 0) { throw "$what failed ($LASTEXITCODE)" }
}

New-Item -ItemType Directory -Force $build | Out-Null
$libs = Join-Path $build 'jniLibs'
$abiArgs = $Abi | ForEach-Object { '-t'; $_ }
Run 'cargo ndk' { cargo ndk @abiArgs -o $libs build --release -p tacit-android }

# Resources and manifest.
$res = Join-Path $build 'res.zip'
$base = Join-Path $build 'base.apk'
Run 'aapt2 compile' { & "$($tools.FullName)\aapt2.exe" compile --dir (Join-Path $app 'res') -o $res }
Run 'aapt2 link' {
    & "$($tools.FullName)\aapt2.exe" link -o $base -I $androidJar --manifest (Join-Path $app 'AndroidManifest.xml') `
        --min-sdk-version $MinSdk --target-sdk-version $TargetSdk --java (Join-Path $build 'gen') $res
}

# Java -> classes -> classes.dex
$classes = Join-Path $build 'classes'
$dex = Join-Path $build 'dex'
New-Item -ItemType Directory -Force $classes, $dex | Out-Null
$sources = @(Join-Path $app 'src'; Join-Path $build 'gen') | ForEach-Object { Get-ChildItem $_ -Recurse -Filter *.java } | ForEach-Object FullName
Run 'javac' { javac -encoding UTF-8 -Xlint:-options -source 8 -target 8 -classpath $androidJar -d $classes @sources }
$classFiles = Get-ChildItem $classes -Recurse -Filter *.class | ForEach-Object FullName
Run 'd8' { & "$($tools.FullName)\d8.bat" --release --min-api $MinSdk --lib $androidJar --output $dex @classFiles }

# classes.dex and the native libraries go into the apk.
Add-Type -AssemblyName System.IO.Compression.FileSystem
$unsigned = Join-Path $build 'unsigned.apk'
Copy-Item $base $unsigned -Force
$zip = [System.IO.Compression.ZipFile]::Open($unsigned, 'Update')
try {
    [void][System.IO.Compression.ZipFileExtensions]::CreateEntryFromFile($zip, (Join-Path $dex 'classes.dex'), 'classes.dex')
    foreach ($a in $Abi) {
        $so = Join-Path $libs "$a\libtacit_android.so"
        [void][System.IO.Compression.ZipFileExtensions]::CreateEntryFromFile($zip, $so, "lib/$a/libtacit_android.so")
    }
} finally { $zip.Dispose() }

# Align and sign. With TS_KEYSTORE and TS_KEYSTORE_PASS set (the release key, alias "tacit")
# the apk can be updated over earlier releases; without them a debug key is created once.
$aligned = Join-Path $build 'aligned.apk'
$apk = Join-Path $build 'tacit.apk'
Run 'zipalign' { & "$($tools.FullName)\zipalign.exe" -f -p 4 $unsigned $aligned }
if ($env:TS_KEYSTORE) {
    $keystore = $env:TS_KEYSTORE
    $pass = $env:TS_KEYSTORE_PASS
    if (-not (Test-Path $keystore) -or -not $pass) { throw 'TS_KEYSTORE needs an existing file and TS_KEYSTORE_PASS' }
} else {
    $keystore = Join-Path $build 'debug.keystore'
    $pass = 'android'
    if (-not (Test-Path $keystore)) {
        Run 'keytool' {
            keytool -genkeypair -keystore $keystore -storepass $pass -keypass $pass -alias debug `
                -keyalg RSA -keysize 2048 -validity 10000 -dname 'CN=tacit Debug'
        }
    }
}
$env:TS_SIGN_PASS = $pass
Run 'apksigner' {
    & "$($tools.FullName)\apksigner.bat" sign --ks $keystore --ks-pass env:TS_SIGN_PASS --key-pass env:TS_SIGN_PASS `
        --out $apk $aligned
}
Write-Host ("built {0} ({1:N0} KB)" -f $apk, ((Get-Item $apk).Length / 1KB))
