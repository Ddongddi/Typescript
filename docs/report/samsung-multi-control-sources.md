# Samsung Multi Control 한/영 키 충돌 조사

조사일: 2026-10-21. 범위: [이슈 #9](https://github.com/iamallyouneed/Tacit/issues/9), Tacit `773b5c9`, 삼성·Microsoft 공식 문서. 실제 연결된 Android에서 증상을 재현하지 않았다. 로컬 PC에는 Multi Control 2.7.1300.0이 설치돼 있으나, 설치 여부는 연결·입력 전달 성공의 증거가 아니다.

## 현재 상태와 다음 확인

- **확인 완료:** E038 한/영 키의 배열별 Windows 가상 키 차이, 실제 삼성 변환 함수의 `Language(122)` → `LeftMenu(56)` 불일치, 삼성 원격 전송 코드까지의 정적 호출 경로.
- **아직 미확인:** 제보자 키보드의 실제 스캔 코드, 공유 중 활성 HKL과 훅 순서, Android가 최종 수신한 이벤트와 언어 전환 결과.
- **다음 실기 비교:** 같은 키보드·Android 입력란에서 Microsoft 한국어 IME와 Tacit을 번갈아 선택한다. 각각 스캔 코드·가상 키·HKL·전환 성공 여부만 기록한다. 입력 내용은 기록하지 않는다.
- **판정 기준:** 한국어 IME에서는 VK_HANGUL로 전환되고 Tacit에서는 VK_RMENU가 되어 실패하면 이 원인과 일치한다. 두 경우 모두 VK_HANGUL인데 실패하면 이 경로로 설명하지 말고 다른 전달 단계를 조사한다.
- **보류한 변경:** 한국어 프로필로 재등록, 전역 키 재주입, 삼성 프로세스 이름 예외. 실기 근거 없이 적용하지 않았다.
- **작업 상태:** 조사만 정리했으며 제품 수정·사용자 설정 변경·이슈 댓글·원격 게시를 하지 않았다.

## 추가 조사 결론: 영문 프로필에서 한/영 키의 정체성이 바뀐다

**물리 스캔 코드 `E038`을 사용하는 한/영 키에서 변환 불일치를 재현했다.** 앞서 의심한 단축키 전역 훅과 별개로 발생한다. 현재 로컬 설정은 `hangulKey=Hangul`, `hangulAction=flip`이므로 이 설정의 전역 훅은 해당 키를 소비하지 않는다.

1. Tacit은 `LANGID_TACIT=0x0409`(en-US)로 등록된다. 한국어 프로필의 IMM32 조합이 글자 단위로 깨지는 문제를 피하려고 의도적으로 선택한 언어다. [guids.rs:15](https://github.com/iamallyouneed/Tacit/blob/773b5c95c821caacccbe48e97b10194180539436/platforms/windows/src/guids.rs#L15), [registry.rs:93](https://github.com/iamallyouneed/Tacit/blob/773b5c95c821caacccbe48e97b10194180539436/platforms/windows/src/registry.rs#L93)
2. Windows의 실제 `MapVirtualKeyExW(E038, MAPVK_VSC_TO_VK_EX, HKL)` 호출은 한국어 HKL `04120412`에서 `0x15/VK_HANGUL`, 영문 HKL `04090409`에서 `0xA5/VK_RMENU`를 반환한다. API 자체가 입력 언어/배열에 따라 변환한다. [Microsoft: MapVirtualKeyExW](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-mapvirtualkeyexw)
3. 설치된 삼성 바이너리의 원격 전송 경로를 IL로 추적했다. `KeyboardHook.HookCallback`은 저수준 이벤트의 vkCode를 보존해 `HandleControlDeviceTypeRemote` → `KeyboardConverter.MapScanCode`로 넘긴다. 중간에 `VK_RMENU`를 한/영으로 보정하지 않는다.
4. **실제 삼성의 순수 변환 함수 `MapScanCode`를 별도 PowerShell 프로세스에서 호출했다.** 같은 스캔 `0x38`·extended=true에 대해 `VK_HANGUL`이면 삼성 프로토콜 `Language=122`, `VK_RMENU`이면 `LeftMenu=56`을 반환했다. 값의 의미는 함께 설치된 `Samsung.MultiControl.Protocol.ScanCodeMap` enum에서 확인했다. Android keycode 숫자라는 뜻은 아니다.
5. 원격 핸들러는 그 변환 결과를 `KeyboardShare` → `SendKeyEvent` → `MCFMessage.SendKeyMessage`로 전달한다. 언어 키 122에는 즉시 down/up을 보내는 별도 처리도 있지만 56에는 적용되지 않는다. 따라서 이 입력 조건에서는 모바일에 전송할 코드 자체가 이미 언어 전환 키가 아니다.

| 동일 물리 키 E038 | Windows 가상 키 | 실제 삼성 변환 결과 |
| --- | --- | --- |
| 한국어 배열 | `0x15 / VK_HANGUL` | `122 / Language` |
| 영문 배열(Tacit 등록 언어) | `0xA5 / VK_RMENU` | `56 / LeftMenu` |

**확정 범위:** 해당 물리 키·배열 조건에서 Windows와 설치된 실제 삼성 변환 함수 사이의 불일치다. 제보자의 키가 같은 E038인지, 실제 공유 중 Windows가 어느 HKL로 이벤트를 만들었는지, Android가 최종 수신한 이벤트는 아직 기록하지 않았다. 전용 한/영 키가 배열과 무관하게 VK_HANGUL을 내는 키보드에는 같은 원인을 일반화하지 않는다.

## 재현 명령과 바이너리 근거

```powershell
powershell -NoProfile -File docs/report/samsung-multi-control/mapping-repro.ps1
```

실제 출력은 위 표와 같다. “같은 물리 키는 같은 언어 전환 키로 보존돼야 한다”는 검사에서 **의도대로 종료 코드 1로 실패**한다: `REPRODUCED: ... (122 -> 56/LeftMenu)`. [layout-probe.ps1](samsung-multi-control/layout-probe.ps1)의 F1 매핑 자체 검사는 통과해 API 사용 오류를 구분했다. [mapping-repro.ps1](samsung-multi-control/mapping-repro.ps1)은 정적으로 확인한 삼성 버전·바이너리 해시와 일치할 때만 순수 변환 함수를 호출한다. 한국어·영문 배열이 모두 로드돼 있어야 하며, 이 재현 스크립트가 직접 배열을 추가하지는 않는다. 소프트웨어 키를 주입하지 않았고, 삼성의 훅·입력 핸들러·앱 진입점은 실행하지 않았다. 호출한 삼성 메서드와 정적 초기화는 사전 생성·조회·정수 분기만 수행하는 것을 먼저 확인했다.

- 실제 바이너리: 설치 패키지의 `SamsungMultiControl/Binary/SamsungMultiControl.exe`, 버전 2.7.1300.0.
- SHA-256: `6AB6A82EBBAF5F85511E5638897FF590DA9B10DC760A3E137A0A0944A7C73A7E`.
- 임시 정적 분석 기록: `temp/issue9-native-findings.md`, `temp/issue9-focused-il.txt`. 임시 파일을 정리해도 재현 스크립트와 이 문서의 결론·메서드 식별자는 남는다. 삼성 바이너리와 IL 덤프를 저장소 배포 파일에 포함하지 않았다.
- 핵심 메서드 식별자: `HookCallback`(0x0600025c), `MapScanCode`(0x06000248), `KeyboardConverter..cctor`(0x0600024b).
- `dnfile`/`dncil`은 조사용 `temp/issue9-tools`에만 설치했다. 제품 의존성은 바꾸지 않았다.

삼성 원격 훅은 정상 원격 경로에서 이미 전달한 키를 소비해 이후 스레드 훅·TSF로 보내지 않는다. 따라서 앞서 본 Tacit의 스레드용 오른쪽 Alt 훅을 이 경로의 주원인으로 잡는 것은 근거가 약하다. 또한 삼성 키보드 콜백에는 injected 비트를 이유로 거부하는 분기가 없었다. 바이너리의 `bInjected` 문자열은 마우스 처리였으므로 “삼성이 합성 키를 무시해서 그렇다”는 추측은 배제한다.

삼성의 한국어 특수 보정 `GetvkParse`는 수신/로컬 재생 경로에만 있다. 그 설정 watcher도 현재 HKL이 아니라 하드웨어 `OverrideKeyboardSubtype`을 읽는다. 이 보정은 위 원격 전송의 불일치를 해결하지 않는다.

## 대응 방향

현재 근거로 가장 직접적인 비교는 Android를 제어하기 전에 PC 입력기를 Microsoft 한국어 IME로 바꿔 같은 키를 시험하는 것이다. 기존 Tacit의 `아무것도 안 함`은 훅 소비 여부를 비교할 수 있지만 영문 HKL 매핑은 바꾸지 않으므로 이 원인을 해결한다고 보장할 수 없다. Shift+Space 등 대체 키는 아래 삼성 DeX 안내에 따른 별도 비교 후보다.

Tacit 전체를 한국어 프로필로 돌리면 기존 IMM32 조합 회귀를 다시 만들 수 있다. 수정은 입력 세션 엔진보다 원격 공유 시의 키 정체성/프로필 호환 경계에서 검토해야 한다. 공유 대상 전환 감지와 실제 훅 순서를 검증하지 않은 채 전역 재주입 훅이나 삼성 프로세스 이름 예외를 추가하지 않았다.

## 1차 코드 조사에서 확인한 사실

1. **단축키 보내기는 원래 키를 전역으로 소비한다.** `global::State::on_key()`는 설정의 동작이 `Send`일 때 `take=true`를 반환한다. 일반 한/영 키는 누를 때, 오른쪽 Alt 같은 modifier는 단독으로 뗄 때 대체 단축키를 보낸다. `proc()`는 소비한 이벤트에 `LRESULT(1)`을 반환하고, 합성한 단축키는 `SENT_BY_US` 태그로 다시 처리하지 않는다. 이 판단에는 앱 정책·세션 일시정지·입력기 활성 프로필·PC/Android 제어 대상이 들어가지 않는다. [global.rs:65–138](https://github.com/iamallyouneed/Tacit/blob/773b5c95c821caacccbe48e97b10194180539436/platforms/windows/src/global.rs#L65), [keymap.rs:129](https://github.com/iamallyouneed/Tacit/blob/773b5c95c821caacccbe48e97b10194180539436/platforms/windows/src/keymap.rs#L129)
2. **기본 동작과 단축키 보내기는 구분해야 한다.** `Settings::default()`의 한/영 동작은 `Flip`이다. 전역 훅은 이 설정과 `Nothing`에서 원래 키를 통과시킨다. 따라서 제보가 기본 설정이었다면 전역 단축키 훅만으로 설명할 수 없다. [policy.rs](https://github.com/iamallyouneed/Tacit/blob/773b5c95c821caacccbe48e97b10194180539436/crates/core/src/store/policy.rs#L283), [global.rs:82](https://github.com/iamallyouneed/Tacit/blob/773b5c95c821caacccbe48e97b10194180539436/platforms/windows/src/global.rs#L82)
3. **오른쪽 Alt에는 별도 스레드 훅이 있다.** `WH_KEYBOARD` 훅이 `Inner::right_alt()`를 호출한다. 여기에 동작이 할당돼 있으면, 실제로 수행할 문서 컨텍스트가 없거나 `act_on()`이 일시정지·앱 정책 때문에 `false`를 반환해도 최종적으로 `true`를 반환해 키를 소비한다. 이 코드는 전역 훅과 별개다. 단, Multi Control이 이 단계에서 키를 수집하는지는 확인되지 않았다. [hook.rs:48](https://github.com/iamallyouneed/Tacit/blob/773b5c95c821caacccbe48e97b10194180539436/platforms/windows/src/hook.rs#L48), [service.rs:385–407](https://github.com/iamallyouneed/Tacit/blob/773b5c95c821caacccbe48e97b10194180539436/platforms/windows/src/service.rs#L385)
4. **기존 설정으로 가로채기를 줄일 수 있다.** 한/영 동작을 `none`으로 두면 일반 한/영 및 오른쪽 Alt의 단독 동작 매칭이 없어져 위 두 경로에서 통과한다. 단, 사용자 지정 `pauseKeys`에 같은 키가 있으면 그것이 우선한다. 한/영 역할을 다른 키(예: F13)로 옮기는 방법도 원래 한/영 키를 비워 준다. 이는 코드에서 확인한 키 전달 동작이며, 실제 Multi Control 문제 해결을 검증했다는 뜻은 아니다. [policy.rs:311](https://github.com/iamallyouneed/Tacit/blob/773b5c95c821caacccbe48e97b10194180539436/crates/core/src/store/policy.rs#L311)

**1차 분석의 범위:** 단축키 전역 훅의 충돌 가능성과 키 설정 분기는 여전히 사실이다. 다만 기본값의 추가 조사에서는 위의 프로필/키 변환 경로가 더 직접적인 증거를 제공한다. 앱 제외나 입력기 일시정지만으로 전역 가로채기가 해제된다고 안내해서는 안 된다.

## 로컬 검증

실기 입력을 보내거나 사용자 설정을 바꾸지 않고 별도 작업 폴더 `temp/issue9-investigation`에서 기존 전역 훅 테스트와 조사용 테스트를 실행했다.

```powershell
$env:CARGO_TARGET_DIR='C:\playground\Typescript\temp\review-target7'
cargo test -p tacit-tip global::tests --locked
```

조사용 두 테스트는 실제 `State::on_key()`를 호출한다. 하나는 `VK_HANGUL=0x15`의 down/up이 `Send`에서 소비되고 `Flip`·`Nothing`에서는 통과하는지, 다른 하나는 한/영 역할을 F13으로 옮기면 원래 한/영·오른쪽 Alt가 통과하는지 확인한다. **기존 4개 + 조사용 2개, 총 6개 통과**(0.06초). 이 테스트는 Samsung 프로그램이나 Android를 실행하는 종단 간 재현이 아니다.

## 확인된 사실

- Multi Control은 Galaxy Book의 내장 또는 외장 키보드·마우스로 Galaxy 휴대폰·태블릿을 제어하며, 포인터를 연결된 기기로 옮긴 후 입력한다. 삼성은 모바일 기기에서 DeX를 사용하는 경우도 지원한다고 설명한다. 이 설명만으로 Windows 쪽의 키 수집 API나 전송 프로토콜은 알 수 없다. [Samsung: Use Multi control to share a keyboard and mouse between Galaxy devices](https://www.samsung.com/us/support/answer/ANS10001485/)
- Windows 저수준 키보드 훅 `LowLevelKeyboardProc`은 이벤트가 스레드 입력 큐에 들어가기 전에 호출된다. 처리한 이벤트에 0이 아닌 값을 반환하면 남은 훅 체인과 대상 창 프로시저로의 전달을 막을 수 있다. 처리하지 않는 이벤트는 `CallNextHookEx`로 넘기라고 Microsoft가 명시한다. 따라서 원래 한/영 이벤트를 훅에서 소비하는 프로그램은 다른 입력 중계 프로그램과 충돌할 수 있다. 마지막 문장은 Windows 계약에서 도출한 가능성이지 Multi Control의 구현이 확인됐다는 뜻은 아니다. [Microsoft: LowLevelKeyboardProc](https://learn.microsoft.com/en-us/windows/win32/winmsg/lowlevelkeyboardproc)
- `SetWindowsHookEx`는 새 훅을 체인의 맨 앞에 설치한다. 따라서 두 프로그램이 같은 훅 체인을 사용한다면 설치·재설치 순서가 관찰 결과에 영향을 줄 수 있다. 후자는 조건부 추론이다. [Microsoft: Hooks Overview](https://learn.microsoft.com/en-us/windows/win32/winmsg/about-hooks)
- `KBDLLHOOKSTRUCT.flags`의 `LLKHF_INJECTED`는 프로세스가 주입한 이벤트라는 뜻이다. `LLKHF_LOWER_IL_INJECTED`는 더 낮은 무결성 수준에서 주입됐다는 뜻이다. 이 비트만으로 Multi Control 제어 대상이 Android인지, 특정 앱이 이벤트를 만들었는지 판별할 수는 없다. [Microsoft: KBDLLHOOKSTRUCT](https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-kbdllhookstruct)
- `SendInput`은 합성 이벤트를 입력 스트림에 직렬로 삽입한다. UIPI에 따라 같거나 낮은 무결성 수준의 앱에만 주입할 수 있으며, 반환값은 삽입한 이벤트 수다. 이 API가 성공하더라도 Multi Control의 전송·Android 언어 전환 성공을 보장하는 계약은 아니다. [Microsoft: SendInput](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput)

## 공식적으로 확인된 대체 언어 전환 키

삼성 공식 **DeX** 안내는 자판 배열을 설정한 뒤 다음 키로 입력 언어를 전환할 수 있다고 설명한다.

- 한/영 키
- Shift+Space
- Control+Space
- 왼쪽 Alt+Shift

설정 경로는 `설정 > 일반 > 하드웨어 키보드 > 언어 변경 단축키`이며, 여기서 일부 단축키를 끌 수 있다. [삼성전자서비스: Dex에서 키보드 입력 언어 추가 및 변경 방법](https://www.samsungsvc.co.kr/solution/961174), [Samsung: How to use Samsung DeX features and fix issues?](https://www.samsung.com/sa_en/support/apps-services/how-can-i-use-the-supported-features-in-samsung-dex/)

**범위 제한:** 이는 DeX·삼성 키보드 안내다. 모든 One UI 버전·Android IME·Multi Control 조합에서 같은 단축키가 통과한다는 근거는 아니다. 실제 제보 환경에서 선택한 키보드, 활성화된 입력 언어, 하드웨어 키보드 설정을 확인한 뒤 비교 실험할 후보로 취급한다.

## 확인되지 않은 점과 판별할 실험

공식 문서만으로는 Multi Control 내부 동작을 확인하지 못했으나, 후속 바이너리 조사로 WH_KEYBOARD_LL과 위 키 매핑은 확인했다. Android 제어 중임을 외부 앱에 알려 주는 공식 공개 API는 여전히 확인하지 못했다. “삼성이 합성 키를 무시한다”는 추측은 확인된 코드와 맞지 않으며, 프로세스 이름만으로 공유 대상을 판별할 수도 없다.

1. 같은 장치·앱·키보드로 Tacit 실행/완전 종료를 비교한다. Tacit 비활성화와 프로세스 종료는 별도로 기록한다.
2. PC와 Android 입력 위치를 각각 기록하고 한/영, Shift+Space, Control+Space, 왼쪽 Alt+Shift를 비교한다. Android의 단축키 설정도 함께 기록한다.
3. 두 프로그램을 반대 순서로 실행해 차이가 있는지 본다. 차이가 나면 훅 순서 가설을 강화하지만, 이 실험만으로 삼성의 실제 API를 확정할 수는 없다.
4. 키 코드·스캔 코드·다운/업·주입 플래그와 Tacit이 소비/통과한 결정을 관찰한다. 실제 입력 내용은 수집하지 않아도 된다.

`아무것도 안 함`(`hangulAction: none`)으로 바꿔 같은 Android 입력란에서 재시험하는 것은 훅 소비와 프로필 매핑 원인을 구분하는 비교다. 사용자 지정 일시정지 키가 같은 물리 키에 할당돼 있다면 그 조건도 별도로 기록한다. 키 역할을 옮기는 설정 역시 원래 이벤트 소비를 줄일 수 있으나 영문 배열에서 이미 Alt로 바뀌는 것을 복원하지는 않는다.

자동 호환을 구현하려면 실제 제어 대상의 전환을 판별할 근거가 더 필요하다. Multi Control 프로세스가 실행 중이라는 사실만으로 Android를 제어 중이라고 판단하면 PC 입력에도 잘못 적용될 수 있다. 원래 한/영 키와 합성 단축키를 무조건 함께 보내는 변경도 PC에서 두 동작을 일으킬 수 있으므로 이번 조사로 정당화되지 않는다.

제품 코드 수정·외부 댓글·Android 왕복 실기 테스트는 수행하지 않았다. 재현 스크립트 두 개는 이 문서 옆에 보존했고, 추가 Rust 테스트와 정적 분석 도구는 임시 작업 폴더에 있다. main의 제품 소스는 그대로다. 실제 삼성 변환 함수까지 재현했지만 제보 환경의 실제 키 이벤트와 Android 수신 결과는 아직 미확인이다.
