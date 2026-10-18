//! The 6-month (26-week, 182-day) curriculum. Every day mixes vocabulary, reading aloud,
//! listening/shadowing, self-expression and business English; the weekly theme drives what
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
    // Phase 1: sounds & survival English
    w("Introducing yourself", "자기소개", "Introducing yourself at work", "be verbs, present simple", "BBC Learning English introduce yourself"),
    w("Daily routines", "하루 일과", "Describing your job and daily tasks", "present simple, frequency adverbs", "VOA Learning English daily routine"),
    w("Family and friends", "가족과 친구", "Talking about colleagues and teams", "have/has, possessives", "English conversation family friends easy"),
    w("Food and restaurants", "음식과 식당", "Business lunch and ordering", "can/could for requests", "ordering food in English restaurant conversation"),
    w("Shopping and money", "쇼핑과 돈", "Prices, quantities and simple numbers", "countable/uncountable, how much/many", "shopping English conversation"),
    w("Directions and transport", "길 찾기와 교통", "Visiting a client office", "prepositions of place, imperatives", "asking for directions English"),
    // Phase 2: everyday conversation
    w("Hobbies and free time", "취미와 여가", "Small talk before a meeting", "like/love + -ing", "small talk English native speakers"),
    w("Travel experiences", "여행 경험", "Business trips and hotels", "past simple", "travel English conversation airport hotel"),
    w("Health and feelings", "건강과 감정", "Calling in sick, rescheduling", "should, feel + adjective", "English at the doctor conversation"),
    w("Weather and plans", "날씨와 계획", "Scheduling meetings", "be going to, will", "making plans in English"),
    w("Past experiences", "지난 경험 이야기", "Talking about your career history", "present perfect", "present perfect English speaking practice"),
    w("Opinions and preferences", "의견과 선호", "Agreeing and disagreeing politely", "I think / in my opinion, comparatives", "giving opinions in English"),
    // Phase 3: storytelling + business foundations
    w("Work and careers", "일과 경력", "Explaining your role and responsibilities", "responsible for, in charge of", "business English job responsibilities"),
    w("Meetings", "회의", "Running and joining meetings", "suggestions: how about, why don't we", "business English meetings phrases"),
    w("Phone and video calls", "전화와 화상회의", "Starting, clarifying and ending calls", "polite questions, could you", "business English phone call video call"),
    w("Telling stories", "이야기하기", "Describing a project result", "past continuous, sequencing words", "storytelling in English speaking"),
    w("Problems and solutions", "문제와 해결", "Reporting problems and proposing fixes", "first conditional", "business English problem solving"),
    w("Presentations", "프레젠테이션", "Presenting data and slides", "signposting language", "business English presentation phrases"),
    w("Emails you speak", "업무 요청과 후속조치", "Requests, reminders and follow-ups", "would you mind, polite requests", "business English follow up phrases"),
    // Phase 4: real-world fluency
    w("Negotiation", "협상", "Negotiating price and terms", "second conditional", "business English negotiation"),
    w("Job interviews", "면접", "Answering interview questions", "past simple vs present perfect", "job interview English answers"),
    w("Culture and society", "문화와 사회", "Cross-cultural communication at work", "relative clauses", "TED talk easy English culture"),
    w("News and current events", "뉴스와 시사", "Discussing market trends", "passive voice", "VOA Learning English news"),
    w("Technology and the future", "기술과 미래", "Pitching a product or idea", "future forms, might/may", "TED talk technology future"),
    w("Debate and persuasion", "토론과 설득", "Persuading stakeholders", "contrast: however, although", "how to persuade in English"),
    w("Final review & real conversation", "총정리와 실전 대화", "Full business conversation simulation", "review of all forms", "English conversation practice native speed"),
];

pub fn phase_of_week(week: u32) -> Phase {
    match week {
        1..=6 => Phase { name_ko: "1단계 · 소리와 생존 영어", goal_ko: "기본 발음과 생존 표현을 입으로 익힙니다.", new_words: 8, sentences: 5 },
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
/// 1 = months 3-4 (off by default, on demand), 2 = months 5-6 (English only; Korean when
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

/// Sentence starters for each week's theme and grammar (same order as `WEEKS`), offered as
/// hints when the learner makes their own sentences with the day's words.
pub const STARTERS: [[&str; 3]; 26] = [
    ["I'm …", "I work at …", "My … is …"],
    ["I usually … every morning.", "I always … after work.", "On weekends, I sometimes …"],
    ["I have a … who …", "My team's … is …", "My best friend has …"],
    ["Could I have …, please?", "Can we … for lunch?", "I'd like to order …"],
    ["How much is …?", "I need some …", "How many … do you have?"],
    ["The … is next to …", "Go straight and turn …", "Take the … to …"],
    ["I love …ing on weekends.", "I like …ing because …", "In my free time, I enjoy …"],
    ["Last year, I went to …", "On my business trip, I …", "I stayed at … and …"],
    ["I feel … today because …", "You should …", "I can't come in today because …"],
    ["Tomorrow, I'm going to …", "I think it will …", "Let's meet on … at …"],
    ["I have … for … years.", "Have you ever …?", "I've never … before."],
    ["I think … is better than …", "In my opinion, …", "I agree, but …"],
    ["I'm responsible for …", "I'm in charge of …", "My main role is to …"],
    ["How about … ?", "Why don't we …?", "Let's start with …"],
    ["Could you … , please?", "Sorry, could you repeat …?", "Thanks for … — talk soon."],
    ["While I was …, …", "First, … Then, … Finally, …", "The project ended with …"],
    ["If we …, we will …", "The problem is that …", "One solution is to …"],
    ["As you can see, …", "Let me move on to …", "This slide shows …"],
    ["Would you mind …ing …?", "Just a quick reminder that …", "I'm following up on …"],
    ["If you … , we would …", "Could you lower … if we …?", "Our best offer is …"],
    ["I have worked as … since …", "In my last job, I …", "My biggest strength is …"],
    ["People who … often …", "In my culture, …", "The company, which …, …"],
    ["It was reported that …", "… is expected to …", "Sales were … by …"],
    ["In the future, we might …", "This app will …", "Our idea may …"],
    ["However, …", "Although …, I believe …", "The main reason is …"],
    ["To sum up, …", "What I learned is …", "Next time, I'll …"],
];

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
/// B2 (level 8) — shown on the dashboard as the pace target.
pub fn target_level_for_day(day: i64, start_level: f32) -> f32 {
    let goal = 8.0f32.max(start_level + 2.0).min(10.0);
    start_level + (goal - start_level) * (day as f32 / TOTAL_DAYS as f32)
}
