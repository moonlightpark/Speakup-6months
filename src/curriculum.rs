//! The 6-month (26-week, 182-day) curriculum. Every day mixes vocabulary, reading aloud,
//! listening/shadowing, self-expression and business Japanese; the weekly theme drives what
//! Claude generates for each of those.

use crate::models::{Profile, today};

pub const TOTAL_DAYS: i64 = 182;
pub const TOTAL_WEEKS: u32 = 26;
/// Weeks that end with a level checkpoint (monthly evaluation).
pub const CHECKPOINT_WEEKS: &[u32] = &[4, 8, 13, 17, 21, 26];

pub struct Week {
    pub theme: &'static str,
    pub theme_ko: &'static str,
    pub business: &'static str,
    pub grammar: &'static str,
    pub listening: &'static str,
}

pub struct Phase {
    pub name_ko: &'static str,
    pub goal_ko: &'static str,
    pub new_words: u32,
    pub sentences: u32,
}

const fn w(theme: &'static str, theme_ko: &'static str, business: &'static str, grammar: &'static str, listening: &'static str) -> Week {
    Week { theme, theme_ko, business, grammar, listening }
}

pub const WEEKS: [Week; 26] = [
    // Phase 1: sounds & survival Japanese
    w("Introducing yourself", "자기소개", "Introducing yourself at work", "です・ます, ～は～です, ～じゃないです", "日本語 自己紹介 会話 初級"),
    w("Daily routines", "하루 일과", "Describing your job and daily tasks", "ます-form for habits, ～時に, ～から～まで", "日本語 一日の生活 初級 リスニング"),
    w("Family and friends", "가족과 친구", "Talking about colleagues and teams", "あります/います, の (possession), family words", "日本語 家族 会話 初級"),
    w("Food and restaurants", "음식과 식당", "Business lunch and ordering", "～をください, ～にします, ～てもいいですか", "日本語 レストラン 注文 会話"),
    w("Shopping and money", "쇼핑과 돈", "Prices, quantities and simple numbers", "numbers and counters, いくら, ～がほしい", "日本語 買い物 会話 初級"),
    w("Directions and transport", "길 찾기와 교통", "Visiting a client office", "て-form sequences (行って、曲がって), location に/で", "日本語 道案内 会話"),
    // Phase 2: everyday conversation
    w("Hobbies and free time", "취미와 여가", "Small talk before a meeting", "～のが好き, ～たり～たり", "日本語 趣味 会話 ネイティブ"),
    w("Travel experiences", "여행 경험", "Business trips and hotels", "past ～ました / plain past ～た", "日本語 旅行 会話 ホテル 空港"),
    w("Health and feelings", "건강과 감정", "Calling in sick, rescheduling", "～んです, ～たほうがいい", "日本語 病院 会話"),
    w("Weather and plans", "날씨와 계획", "Scheduling meetings", "～つもり, ～予定, ～でしょう", "日本語 予定 会話 週末"),
    w("Past experiences", "지난 경험 이야기", "Talking about your career history", "～たことがある", "日本語 経験 話す 練習"),
    w("Opinions and preferences", "의견과 선호", "Agreeing and disagreeing politely", "～と思います, ～より～のほうが", "日本語 意見 言い方"),
    // Phase 3: storytelling + business foundations
    w("Work and careers", "일과 경력", "Explaining your role and responsibilities", "～を担当しています, ～ています", "ビジネス日本語 仕事 説明"),
    w("Meetings", "회의", "Running and joining meetings", "suggestions: ～ましょうか, ～たらどうですか", "ビジネス日本語 会議 表現"),
    w("Phone and video calls", "전화와 화상회의", "Starting, clarifying and ending calls", "keigo basics: ～いただけますか, お～します", "ビジネス日本語 電話 敬語"),
    w("Telling stories", "이야기하기", "Describing a project result", "～てから, ～とき, それから", "日本語 話し方 エピソード"),
    w("Problems and solutions", "문제와 해결", "Reporting problems and proposing fixes", "conditionals ～たら / ～ば", "ビジネス日本語 報告 相談"),
    w("Presentations", "프레젠테이션", "Presenting data and slides", "～について, ～によると, まず・次に", "日本語 プレゼン 表現"),
    w("Requests and follow-ups", "업무 요청과 후속조치", "Requests, reminders and follow-ups", "～ていただけませんか, ～でしょうか", "ビジネス日本語 依頼 メール 電話"),
    // Phase 4: real-world fluency
    w("Negotiation", "협상", "Negotiating price and terms", "～なら, ～ば～ほど", "ビジネス日本語 交渉"),
    w("Job interviews", "면접", "Answering interview questions", "honorific and humble keigo (尊敬語・謙譲語)", "日本語 面接 受け答え"),
    w("Culture and society", "문화와 사회", "Cross-cultural communication at work", "noun-modifying clauses (連体修飾)", "日本 文化 違い 韓国"),
    w("News and current events", "뉴스와 시사", "Discussing market trends", "passive ～られる, hearsay ～そうです", "NHK NEWS WEB EASY"),
    w("Technology and the future", "기술과 미래", "Pitching a product or idea", "～かもしれない, ～だろう", "日本語 テクノロジー 未来 ニュース"),
    w("Debate and persuasion", "토론과 설득", "Persuading stakeholders", "contrast: ～けれども, ～のに, ～に対して", "日本語 討論 意見"),
    w("Final review & real conversation", "총정리와 실전 대화", "Full business conversation simulation", "review of all forms", "日本語 会話 ネイティブ スピード"),
];

pub fn phase_of_week(week: u32) -> Phase {
    match week {
        1..=6 => Phase { name_ko: "1단계 · 소리와 생존 일본어", goal_ko: "기본 발음과 생존 표현을 입으로 익힙니다.", new_words: 8, sentences: 5 },
        7..=12 => Phase { name_ko: "2단계 · 일상 회화 확장", goal_ko: "일상 주제로 3~4문장 이상 말할 수 있게 됩니다.", new_words: 10, sentences: 5 },
        13..=19 => Phase { name_ko: "3단계 · 스토리텔링과 비즈니스 기초", goal_ko: "경험을 이야기하고 업무 상황에서 의사를 전달합니다.", new_words: 10, sentences: 6 },
        _ => Phase { name_ko: "4단계 · 실전 유창성", goal_ko: "원어민 속도로 듣고 막힘 없이 대화합니다.", new_words: 12, sentences: 6 },
    }
}

/// 1-based day number of the 6-month program (1..=182).
pub fn day_number(p: &Profile) -> i64 {
    ((today() - p.start_date).num_days() + 1).clamp(1, TOTAL_DAYS)
}

/// How much Korean support the screens give, fading over the program so the learner stops
/// translating in their head: 0 = months 1-2 (직독직해 and Korean always shown),
/// 1 = months 3-4 (off by default, on demand), 2 = months 5-6 (Japanese only; Korean when
/// checking the answer).
pub fn support_stage(day: i64) -> u8 {
    match day {
        ..=60 => 0,
        61..=120 => 1,
        _ => 2,
    }
}

pub fn week_number(p: &Profile) -> u32 {
    (((day_number(p) - 1) / 7) as u32 + 1).min(TOTAL_WEEKS)
}

/// Sentence starters for each week's theme and grammar (same order as `WEEKS`, from
/// `assets/ja/misc.json`), offered as hints when the learner makes their own sentences with the
/// day's words.
pub fn starters(week: u32) -> [&'static str; 3] {
    let all = &crate::content::misc().starters;
    match all.get((week.clamp(1, TOTAL_WEEKS) - 1) as usize) {
        Some([a, b, c]) => [a.as_str(), b.as_str(), c.as_str()],
        None => ["…", "…", "…"],
    }
}

pub fn week(n: u32) -> &'static Week {
    &WEEKS[(n.clamp(1, TOTAL_WEEKS) - 1) as usize]
}

pub fn program_finished(p: &Profile) -> bool {
    (today() - p.start_date).num_days() + 1 > TOTAL_DAYS
}

/// The checkpoint the learner is due for, if any: the latest checkpoint week that has been
/// reached (on its last 2 days or later) but not yet taken.
pub fn checkpoint_due(p: &Profile) -> Option<u32> {
    let day = day_number(p);
    CHECKPOINT_WEEKS
        .iter()
        .copied()
        .filter(|w| day >= (*w as i64) * 7 - 1 && *w > p.last_checkpoint_week)
        .max()
}

pub fn next_checkpoint(p: &Profile) -> Option<u32> {
    CHECKPOINT_WEEKS.iter().copied().find(|w| *w > p.last_checkpoint_week)
}

/// The level a learner should be at for this point of the program if they are to finish at
/// N2 (level 8) — shown on the dashboard as the pace target.
pub fn target_level_for_day(day: i64, start_level: f32) -> f32 {
    let goal = 8.0f32.max(start_level + 2.0).min(10.0);
    start_level + (goal - start_level) * (day as f32 / TOTAL_DAYS as f32)
}
