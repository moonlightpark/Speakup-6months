//! Built-in English content used for the placement test and as an offline fallback when
//! Claude is unavailable (no API key / network error). Claude-generated lessons are preferred.

use crate::models::*;

/// Placement / checkpoint reading sentences, easiest first (roughly A1 → C1).
pub const PLACEMENT: &[(&str, &str)] = &[
    ("Hello, my name is Alex. Nice to meet you.", "안녕하세요, 제 이름은 알렉스예요. 만나서 반가워요."),
    ("I usually get up at seven and have coffee for breakfast.", "저는 보통 7시에 일어나서 아침으로 커피를 마셔요."),
    ("Last weekend I went to the beach with my friends.", "지난 주말에 친구들과 해변에 갔어요."),
    ("If it rains tomorrow, we'll have to move the meeting online.", "내일 비가 오면 회의를 온라인으로 옮겨야 해요."),
    ("I've been working on this project for about three months now.", "이 프로젝트를 한 3개월째 하고 있어요."),
    ("Had we known about the delay, we would have adjusted the schedule accordingly.", "지연을 알았더라면 일정을 그에 맞게 조정했을 거예요."),
];

pub const PLACEMENT_QUESTION: (&str, &str) = (
    "Please introduce yourself. What do you do, and why do you want to learn English?",
    "자기소개를 해주세요. 어떤 일을 하시고, 왜 영어를 배우고 싶으신가요?",
);

type W = (&'static str, &'static str, &'static str, &'static str);
type S = (&'static str, &'static str, &'static str);

const WORDS: [&[W]; 4] = [
    &[
        ("introduce", "소개하다", "Let me introduce myself.", "제 소개를 할게요."),
        ("usually", "보통", "I usually walk to work.", "저는 보통 걸어서 출근해요."),
        ("colleague", "동료", "My colleague helped me a lot.", "동료가 많이 도와줬어요."),
        ("order", "주문하다", "Can I order now?", "지금 주문해도 될까요?"),
        ("receipt", "영수증", "Could I have a receipt, please?", "영수증 주시겠어요?"),
        ("straight", "곧장", "Go straight and turn left.", "곧장 가서 왼쪽으로 도세요."),
        ("weekend", "주말", "What did you do last weekend?", "지난 주말에 뭐 했어요?"),
        ("favorite", "가장 좋아하는", "My favorite food is kimchi stew.", "제가 가장 좋아하는 음식은 김치찌개예요."),
        ("busy", "바쁜", "I'm busy this morning.", "오늘 아침엔 바빠요."),
        ("expensive", "비싼", "This bag is too expensive.", "이 가방은 너무 비싸요."),
        ("recommend", "추천하다", "What do you recommend?", "뭘 추천하세요?"),
        ("schedule", "일정", "Let me check my schedule.", "제 일정을 확인해 볼게요."),
        ("nearby", "근처에", "Is there a bank nearby?", "근처에 은행이 있나요?"),
        ("arrive", "도착하다", "What time does the train arrive?", "기차는 몇 시에 도착해요?"),
        ("hungry", "배고픈", "I'm getting hungry.", "슬슬 배고파지네요."),
        ("available", "시간이 되는", "Are you available tomorrow?", "내일 시간 되세요?"),
    ],
    &[
        ("experience", "경험", "It was a great experience.", "정말 좋은 경험이었어요."),
        ("prefer", "선호하다", "I prefer tea to coffee.", "저는 커피보다 차가 좋아요."),
        ("actually", "사실은", "Actually, I've never been there.", "사실 거기 가본 적이 없어요."),
        ("reschedule", "일정을 변경하다", "Can we reschedule our call?", "통화 일정을 바꿀 수 있을까요?"),
        ("opinion", "의견", "In my opinion, it's worth it.", "제 생각엔 그럴 가치가 있어요."),
        ("forecast", "예보", "The forecast says it'll rain.", "예보에 비가 온대요."),
        ("comfortable", "편안한", "This hotel is really comfortable.", "이 호텔 정말 편해요."),
        ("disagree", "동의하지 않다", "I'm afraid I disagree.", "죄송하지만 동의하지 않아요."),
        ("journey", "여정", "How was your journey?", "오시는 길은 어떠셨어요?"),
        ("relax", "쉬다", "I like to relax at home on Sundays.", "일요일엔 집에서 쉬는 걸 좋아해요."),
        ("ever", "한 번이라도", "Have you ever tried surfing?", "서핑 해본 적 있어요?"),
        ("probably", "아마", "I'll probably stay home.", "아마 집에 있을 거예요."),
        ("feel like", "~하고 싶다", "I feel like eating out tonight.", "오늘 밤엔 외식하고 싶어요."),
        ("check in", "체크인하다", "I'd like to check in, please.", "체크인하고 싶어요."),
        ("compare", "비교하다", "Let's compare the two options.", "두 가지 옵션을 비교해 봐요."),
        ("improve", "향상시키다", "I want to improve my English.", "영어 실력을 늘리고 싶어요."),
    ],
    &[
        ("responsible for", "~을 담당하는", "I'm responsible for marketing.", "저는 마케팅을 담당하고 있어요."),
        ("agenda", "안건", "Let's go over today's agenda.", "오늘 안건을 살펴봅시다."),
        ("clarify", "명확히 하다", "Could you clarify that point?", "그 부분을 명확히 해주시겠어요?"),
        ("deadline", "마감일", "The deadline is next Friday.", "마감은 다음 주 금요일이에요."),
        ("issue", "문제", "We have an issue with the server.", "서버에 문제가 있어요."),
        ("solution", "해결책", "I think I found a solution.", "해결책을 찾은 것 같아요."),
        ("follow up", "후속 조치하다", "I'll follow up by email.", "이메일로 후속 연락 드릴게요."),
        ("briefly", "간단히", "Let me briefly explain.", "간단히 설명드릴게요."),
        ("figure", "수치", "These figures show strong growth.", "이 수치들은 큰 성장을 보여줘요."),
        ("suggest", "제안하다", "I suggest we start now.", "지금 시작하는 걸 제안해요."),
        ("on track", "순조롭게 진행 중인", "The project is on track.", "프로젝트는 순조롭게 진행 중이에요."),
        ("priority", "우선순위", "Safety is our top priority.", "안전이 최우선이에요."),
        ("eventually", "결국", "Eventually, we fixed the bug.", "결국 버그를 고쳤어요."),
        ("handle", "처리하다", "I'll handle the client.", "고객은 제가 처리할게요."),
        ("mention", "언급하다", "As I mentioned earlier,", "앞서 말씀드렸듯이,"),
        ("update", "최신 소식", "Here's a quick update.", "간단히 현황을 공유드릴게요."),
    ],
    &[
        ("negotiate", "협상하다", "We need to negotiate the price.", "가격을 협상해야 해요."),
        ("compromise", "타협", "Let's find a compromise.", "타협점을 찾아봅시다."),
        ("strength", "강점", "My biggest strength is teamwork.", "제 가장 큰 강점은 팀워크예요."),
        ("trend", "추세", "This trend will continue.", "이 추세는 계속될 거예요."),
        ("convince", "설득하다", "How can we convince them?", "어떻게 그들을 설득할 수 있을까요?"),
        ("perspective", "관점", "From my perspective, it's risky.", "제 관점에선 위험해요."),
        ("benefit", "이점", "The main benefit is speed.", "주된 이점은 속도예요."),
        ("however", "하지만", "However, there is one problem.", "하지만 한 가지 문제가 있어요."),
        ("in terms of", "~의 면에서", "In terms of cost, it's better.", "비용 면에서 더 나아요."),
        ("approach", "접근법", "Let's try a different approach.", "다른 접근법을 시도해 봐요."),
        ("terms", "조건", "We accept your terms.", "그 조건을 받아들이겠습니다."),
        ("achieve", "달성하다", "We achieved our goal.", "목표를 달성했어요."),
        ("challenge", "도전 과제", "It was a real challenge.", "정말 힘든 과제였어요."),
        ("impact", "영향", "AI will have a huge impact.", "AI는 엄청난 영향을 줄 거예요."),
        ("whereas", "반면에", "I like summer, whereas she likes winter.", "저는 여름이 좋은데 그녀는 겨울을 좋아해요."),
        ("overall", "전반적으로", "Overall, it went well.", "전반적으로 잘 됐어요."),
    ],
];

const SENTENCES: [&[S]; 4] = [
    &[
        ("Nice to meet you. I'm from Seoul.", "만나서 반가워요. 저는 서울에서 왔어요.", "meet you 를 '미츄'처럼 이어서 발음하세요."),
        ("I work at a tech company.", "저는 IT 회사에서 일해요.", "work at a 를 하나로 이어 읽으세요."),
        ("Could I get a glass of water, please?", "물 한 잔 주시겠어요?", "Could I 를 '쿠다이'처럼 연결하세요."),
        ("How much is this?", "이거 얼마예요?", "much is 를 이어서 발음하세요."),
        ("Excuse me, where is the subway station?", "실례합니다, 지하철역이 어디예요?", "where is 의 강세는 where 에."),
        ("I usually go to bed around eleven.", "저는 보통 11시쯤 자요.", "go to 의 to 는 약하게 '터'."),
        ("I have a younger brother.", "저는 남동생이 하나 있어요.", "have a 를 '해버'처럼 연결하세요."),
        ("What do you do for a living?", "무슨 일 하세요?", "What do you 를 '와루유'처럼 빠르게."),
        ("Let me think about it.", "생각해 볼게요.", "think 의 th 는 혀를 이 사이에."),
        ("Can you say that again, please?", "다시 한 번 말씀해 주시겠어요?", "that again 을 이어서."),
    ],
    &[
        ("I've never been to Europe.", "유럽에 가본 적이 없어요.", "I've 의 v 소리를 살리세요."),
        ("What do you usually do on weekends?", "주말엔 보통 뭐 하세요?", "on weekends 의 s 끝소리를 살리세요."),
        ("I'm not feeling very well today.", "오늘 몸이 좀 안 좋아요.", "feeling 의 l 을 분명히."),
        ("Are you free this Friday?", "이번 금요일에 시간 돼요?", "Friday 의 F 는 윗니로 아랫입술."),
        ("I'd rather stay home tonight.", "오늘 밤엔 차라리 집에 있을래요.", "I'd rather 를 연결해서."),
        ("That sounds like a great idea.", "좋은 생각 같아요.", "sounds like 를 이어서."),
        ("I see your point, but I think differently.", "무슨 말인지 알겠지만 저는 다르게 생각해요.", "but 뒤에서 살짝 끊어 읽으세요."),
        ("The weather is supposed to be nice tomorrow.", "내일 날씨가 좋을 거래요.", "supposed to 는 '서포스트'처럼."),
        ("It was the best trip I've ever had.", "지금까지 했던 여행 중 최고였어요.", "best trip 의 t 는 거의 안 들려요."),
        ("I'm planning to visit my parents.", "부모님 댁에 갈 계획이에요.", "planning to 를 이어서."),
    ],
    &[
        ("Let's get started with today's meeting.", "오늘 회의를 시작하죠.", "get started 의 t 는 약하게."),
        ("Could you walk me through the plan?", "계획을 차근차근 설명해 주시겠어요?", "walk 의 l 은 묵음."),
        ("Sorry, you broke up a little. Could you repeat that?", "죄송한데 소리가 좀 끊겼어요. 다시 말씀해 주시겠어요?", "broke up 을 이어서."),
        ("We're running a bit behind schedule.", "일정이 조금 늦어지고 있어요.", "a bit 을 '어빗'처럼."),
        ("I'll send you the file by the end of the day.", "오늘 중으로 파일 보내드릴게요.", "end of the 를 이어서."),
        ("As you can see on this slide, sales went up.", "이 슬라이드에서 보시듯이 매출이 올랐어요.", "went up 을 '웬텁'처럼."),
        ("What if we split the task into two parts?", "작업을 두 부분으로 나누면 어떨까요?", "What if 를 '와리프'처럼."),
        ("Just to clarify, the deadline is Monday, right?", "확인차 말씀드리면 마감이 월요일 맞죠?", "right 는 올려서 질문처럼."),
        ("I'm in charge of the new project.", "제가 새 프로젝트를 맡고 있어요.", "in charge of 를 한 덩어리로."),
        ("Let me get back to you on that.", "그 건은 다시 연락드릴게요.", "get back to 를 연결해서."),
    ],
    &[
        ("That's a fair point, but we need to consider the cost.", "일리 있는 말씀이지만 비용을 고려해야 해요.", "fair point 에 강세."),
        ("If we lowered the price, would you sign today?", "가격을 낮추면 오늘 서명하시겠어요?", "would you 를 '우쥬'처럼."),
        ("I'm confident I can make a strong contribution.", "제가 크게 기여할 수 있다고 확신해요.", "contribution 의 강세는 bu."),
        ("The data suggests that demand will keep growing.", "데이터는 수요가 계속 늘 거라고 보여줘요.", "suggests that 의 ts 를 살리세요."),
        ("Although it's risky, the potential reward is huge.", "위험하긴 하지만 잠재적 보상이 커요.", "Although 의 th 는 유성음."),
        ("Let me put it another way.", "다르게 말씀드려 볼게요.", "put it 을 '푸릿'처럼."),
        ("We're willing to be flexible on delivery dates.", "납품 일정은 유연하게 조정할 수 있어요.", "willing 의 w 를 분명히."),
        ("To sum up, this plan saves both time and money.", "요약하면, 이 계획은 시간과 돈을 모두 절약해요.", "sum up 을 이어서."),
        ("I couldn't agree more.", "전적으로 동의해요.", "couldn't 의 t 는 거의 안 들려요."),
        ("Technology is changing the way we work.", "기술이 우리가 일하는 방식을 바꾸고 있어요.", "the way we 를 부드럽게."),
    ],
];

const QUESTIONS: [&[(&str, &str, &str)]; 4] = [
    &[
        ("Tell me about your typical day.", "당신의 평범한 하루에 대해 말해 주세요.", "I usually wake up at seven. I take the subway to work, and I have lunch with my colleagues. After work, I go to the gym or watch Netflix."),
        ("What is your favorite food and why?", "가장 좋아하는 음식과 그 이유는?", "My favorite food is kimchi stew because it's spicy and warm. My mom makes the best one."),
    ],
    &[
        ("Tell me about a memorable trip you took.", "기억에 남는 여행 이야기를 해주세요.", "A few years ago, I went to Jeju Island with my family. We hiked Hallasan and ate a lot of seafood. It was the best trip I've ever had."),
        ("Do you prefer the city or the countryside? Why?", "도시와 시골 중 어디가 더 좋나요? 왜죠?", "I prefer the city because it's convenient. However, I love visiting the countryside on weekends to relax."),
    ],
    &[
        ("Describe your job and your main responsibilities.", "당신의 업무와 주요 책임을 설명해 주세요.", "I work as a project manager. I'm responsible for planning schedules, running meetings, and making sure we meet our deadlines."),
        ("Tell me about a problem you solved at work.", "직장에서 해결한 문제에 대해 이야기해 주세요.", "Last year, our app kept crashing. I organized a small team, we found the cause in two days, and the crash rate dropped by ninety percent."),
    ],
    &[
        ("How will AI change the way people work?", "AI는 사람들의 일하는 방식을 어떻게 바꿀까요?", "I think AI will take over repetitive tasks, so people can focus on creative work. However, we'll need to keep learning new skills."),
        ("What is your biggest strength, and how has it helped you?", "당신의 가장 큰 강점은 무엇이고 어떻게 도움이 되었나요?", "My biggest strength is that I stay calm under pressure. When our launch was delayed, I helped the team prioritize and we still delivered on time."),
    ],
];

const SCENARIOS: [&[(&str, &str, &str, &str, &str, &[(&str, &str)])]; 4] = [
    &[(
        "First day at a new office", "새 회사 첫 출근, 옆자리 동료와 인사를 나눕니다.", "Emma, a friendly coworker", "a new employee",
        "Hi! You must be the new team member. I'm Emma. Welcome aboard!",
        &[("Nice to meet you, Emma.", "만나서 반가워요, 엠마."), ("I'm on the marketing team.", "저는 마케팅 팀이에요."), ("Where is the meeting room?", "회의실이 어디예요?")],
    )],
    &[(
        "Rescheduling a meeting", "고객사 담당자에게 전화해 회의 일정을 바꿉니다.", "Mr. Brown, a client", "a project coordinator",
        "Hello, this is David Brown speaking. How can I help you?",
        &[("I'm calling about our meeting on Thursday.", "목요일 회의 건으로 전화드렸어요."), ("Would Friday work for you?", "금요일 괜찮으세요?"), ("Sorry for the inconvenience.", "불편을 드려 죄송해요.")],
    )],
    &[(
        "Weekly team meeting", "주간 회의에서 프로젝트 현황을 공유합니다.", "Sarah, the team lead", "a team member presenting an update",
        "Okay everyone, let's get started. Could you give us a quick update on your project?",
        &[("Sure. We're on track.", "네, 순조롭게 진행 중이에요."), ("We had one issue with the server.", "서버 문제가 하나 있었어요."), ("I'll follow up by Friday.", "금요일까지 후속 조치하겠습니다.")],
    )],
    &[(
        "Price negotiation", "공급업체와 단가를 협상합니다.", "Michael, a supplier's sales manager", "a purchasing manager",
        "Thanks for meeting with us. So, have you had a chance to review our proposal?",
        &[("The price is a bit higher than we expected.", "가격이 예상보다 조금 높네요."), ("If we order more, could you offer a discount?", "더 많이 주문하면 할인해 주실 수 있나요?"), ("Let's find a compromise.", "타협점을 찾아봅시다.")],
    )],
];

fn phase_index(week: u32) -> usize {
    match week {
        1..=6 => 0,
        7..=12 => 1,
        13..=19 => 2,
        _ => 3,
    }
}

/// High-frequency spoken chunks per lesson.
pub const CHUNKS_PER_DAY: usize = 4;

/// Builds an offline lesson for the given week, skipping words/sentences the learner already has.
pub fn builtin_lesson(week: u32, day: i64, known: &[StudyItem], past: &[Lesson], n_words: usize, n_sent: usize) -> Lesson {
    let p = phase_index(week);
    let knows = |t: &str| known.iter().any(|i| i.text.eq_ignore_ascii_case(t));
    // Fall back to earlier phases once a phase's pool is used up.
    let words: Vec<LessonWord> = (0..=p)
        .rev()
        .flat_map(|i| WORDS[i].iter())
        .filter(|w| !knows(w.0))
        .take(n_words)
        .map(|w| LessonWord { word: w.0.into(), meaning_ko: w.1.into(), example: w.2.into(), example_ko: w.3.into() })
        .collect();
    let sentences: Vec<LessonSentence> = (0..=p)
        .rev()
        .flat_map(|i| SENTENCES[i].iter())
        .filter(|s| !knows(s.0))
        .take(n_sent)
        .map(|s| LessonSentence { text: s.0.into(), ko: s.1.into(), tip_ko: s.2.into(), chunks: Vec::new() })
        .collect();
    // Prefer a question / scenario that hasn't been used yet (any phase), then rotate.
    let used_q = |q: &str| past.iter().any(|l| l.self_expression.question == q);
    let used_s = |s: &str| past.iter().any(|l| l.business.title == s);
    let q = (0..4)
        .map(|k| (p + k) % 4)
        .flat_map(|i| QUESTIONS[i].iter())
        .find(|q| !used_q(q.0))
        .copied()
        .unwrap_or(QUESTIONS[p][(day as usize) % QUESTIONS[p].len()]);
    let sc = (0..4)
        .map(|k| (p + k) % 4)
        .flat_map(|i| SCENARIOS[i].iter())
        .find(|s| !used_s(s.0))
        .copied()
        .unwrap_or(SCENARIOS[p][(day as usize) % SCENARIOS[p].len()]);
    let wk = crate::curriculum::week(week);
    Lesson {
        date: Some(today()),
        source: "built-in".into(),
        words,
        sentences,
        self_expression: SelfExpression {
            question: q.0.into(),
            question_ko: q.1.into(),
            useful_phrases: Vec::new(),
            model_answer: q.2.into(),
        },
        business: BusinessScenario {
            title: sc.0.into(),
            scenario_ko: sc.1.into(),
            ai_role: sc.2.into(),
            user_role: sc.3.into(),
            opening_line: sc.4.into(),
            key_phrases: sc.5.iter().map(|(t, k)| Phrase { text: (*t).into(), ko: (*k).into() }).collect(),
            guide: Vec::new(),
        },
        listening: ListeningPlan {
            search_queries: vec![wk.listening.into()],
            focus_ko: "자막 한 줄씩 듣고 바로 따라 말하세요(쉐도잉).".into(),
        },
        expressions: crate::expressions::pick_for_today(day, knows)
            .into_iter()
            .map(|e| LessonWord { word: e.0.into(), meaning_ko: e.1.into(), example: e.2.into(), example_ko: e.3.into() })
            .collect(),
        phrases: crate::expressions::pick_chunks(CHUNKS_PER_DAY, knows)
            .into_iter()
            .map(|e| LessonWord { word: e.0.into(), meaning_ko: e.1.into(), example: e.2.into(), example_ko: e.3.into() })
            .collect(),
    }
}

/// Role-play collection (like Learna's scene collections). The AI opens each scene.
pub const ROLEPLAYS: &[(&str, &str, &str, &str)] = &[
    ("Job interview", "외국계 회사 면접을 봅니다.", "a hiring manager interviewing a candidate", "a job candidate"),
    ("Ordering at a restaurant", "레스토랑에서 주문하고 요청사항을 말합니다.", "a friendly waiter", "a customer"),
    ("Hotel check-in", "호텔에 체크인하며 방과 시설을 묻습니다.", "a hotel front desk clerk", "a guest"),
    ("At the airport", "공항 체크인 카운터에서 수하물과 좌석을 처리합니다.", "an airline check-in agent", "a traveler"),
    ("Returning an item", "산 물건을 환불·교환합니다.", "a store clerk", "a customer"),
    ("Doctor's appointment", "병원에서 증상을 설명합니다.", "a doctor", "a patient"),
    ("Networking small talk", "콘퍼런스 네트워킹 행사에서 처음 만난 사람과 스몰토크합니다.", "a professional you just met at a conference", "an attendee"),
    ("Team meeting", "팀 회의에서 의견을 내고 업무를 나눕니다.", "a team manager running a meeting", "a team member"),
    ("Client phone call", "고객에게 전화해 납기 지연을 설명합니다.", "an unhappy client", "an account manager"),
    ("Salary negotiation", "연봉 협상을 합니다.", "an HR manager", "an employee asking for a raise"),
    ("Making friends", "새로 이사 온 동네 이웃과 친해집니다.", "a friendly neighbor", "a new neighbor"),
    ("Asking for directions", "길을 잃어 현지인에게 길을 묻습니다.", "a local person on the street", "a tourist"),
];

/// Offline passages for speed reading (used when Claude isn't available).
pub const SPEED_PASSAGES: &[(&str, &str)] = &[
    ("My Morning", "I usually wake up at seven. First, I drink a big glass of water. Then I take a quick shower and get dressed. I eat toast and eggs for breakfast, and I check the news on my phone. At eight, I leave home and take the subway to work. It's crowded, but I listen to English podcasts, so the time goes by fast."),
    ("A Busy Day at Work", "Today was really busy. In the morning, I had two meetings back to back. After lunch, a client called about a problem with their order, so I had to fix it right away. I finally finished my report at six. I was tired, but I felt good because the whole team worked together and we got everything done on time."),
    ("Weekend Plans", "This weekend, I'm planning to visit my parents. They live in a small town about two hours from Seoul. My mom always cooks way too much food, and my dad loves to show me his garden. On Sunday, I want to go hiking if the weather is nice. Then I'll come back home and get ready for the week."),
    ("Learning English", "When I started learning English, I was too shy to speak. I could read pretty well, but listening was really hard. So I changed my routine. Now I listen to native speakers every day, I repeat what they say out loud, and I talk with an AI tutor. Little by little, I'm getting faster, and I'm not afraid to make mistakes anymore."),
];

/// Well-known channels with native-speaker teaching content (opened as YouTube searches so the
/// links never go stale).
pub const CHANNELS: &[(&str, &str)] = &[
    ("BBC Learning English", "영국 BBC 공식 · 6 Minute English, 뉴스 리뷰"),
    ("VOA Learning English", "미국 VOA · 느린 속도 뉴스와 회화"),
    ("Rachel's English", "미국식 발음·연음 집중"),
    ("English with Lucy", "영국식 발음, 어휘, 표현"),
    ("Speak English With Vanessa", "미국 일상 회화"),
    ("Business English Pod", "비즈니스 회화 상황별"),
    ("TED-Ed", "짧은 강연, 원어민 자연 속도"),
];

pub fn youtube_search_url(query: &str) -> String {
    let q: String = query
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => (b as char).to_string(),
            b' ' => "+".into(),
            _ => format!("%{b:02X}"),
        })
        .collect();
    format!("https://www.youtube.com/results?search_query={q}")
}
