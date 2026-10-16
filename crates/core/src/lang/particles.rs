//! Korean particles and endings that attach straight to a (loan)word.

use std::collections::HashSet;
use std::sync::LazyLock;

use crate::lang::fnv::FnvBuild;

/// Particles and endings that commonly attach directly to a (loan)word.
const ENDING_LIST: &str = "에 에서 에게 에는 에서는 에도 에서도 의 을 를 은 는 이 가 도 로 으로 와 과 랑 이랑 하고 한테 까지 부터 만 보다 \
처럼 이나 나 이야 야 이에요 예요 이다 입니다 인데 이라 이라고 라고 으로는 로는 으로도 로도 이고 이지 이랑은 \
했어 했다 했어요 했는데 했고 했지 했음 해서 해야 하는 하면 하고 하기 하게 하다 해 해요 합니다 할 한 함 하자 해줘 해봐 \
됐어 됐다 된 되는 되면 돼 돼요 됩니다 같은 같아 적 적인 적으로 스럽게 같이 들 들이 들을 들은 에요 이요 를요 을요 \
같은건 같은거 같은게 같은걸 같은데 같다 같네 같아서 같은지 같고 같아요 인가 인지 인데요 인건 인게 인거 \
이라는 라는 이란 에선 으론 로서 으로서";

/// Particles that follow a single letter (x에, n이, a를, X축의 X): case particles only, not
/// endings like 해/한, so short English (ago) doesn't split.
const SINGLE_LETTER_PARTICLES: &str = "에 에서 에게 에는 에서는 에도 의 을 를 은 는 이 가 도 로 으로 와 과 랑 이랑 하고 한테 까지 부터 만 보다 \
처럼 이나 나 이라 이라고 라고 이랑은 으로는 로는 같은 같이";

static ENDINGS: LazyLock<HashSet<&'static str, FnvBuild>> =
    LazyLock::new(|| ENDING_LIST.split_whitespace().collect());
static SINGLE_LETTER: LazyLock<HashSet<&'static str, FnvBuild>> =
    LazyLock::new(|| SINGLE_LETTER_PARTICLES.split_whitespace().collect());

pub fn is_ending(s: &str) -> bool {
    ENDINGS.contains(s)
}

pub fn is_single_letter_particle(s: &str) -> bool {
    SINGLE_LETTER.contains(s)
}
