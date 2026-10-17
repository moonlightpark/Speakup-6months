//! Everyday native expressions and slang, old and new, for the offline lessons (Claude adds
//! fresh ones to generated lessons). (expression, 뜻, example, 예문 해석)

pub type Expr = (&'static str, &'static str, &'static str, &'static str);

/// Expressions that have been around for decades and are still heard every day.
pub const CLASSIC: &[Expr] = &[
    ("What's up?", "잘 지내? / 무슨 일이야?", "Hey, what's up? I haven't seen you in ages.", "야, 잘 지냈어? 진짜 오랜만이다."),
    ("My bad.", "내 실수야, 미안.", "Oops, my bad. I thought this was my seat.", "앗, 미안. 내 자리인 줄 알았어."),
    ("No worries.", "괜찮아, 신경 쓰지 마.", "No worries, I'll wait for you here.", "괜찮아, 여기서 기다릴게."),
    ("Hang out", "어울려 놀다", "Do you want to hang out this weekend?", "이번 주말에 같이 놀래?"),
    ("Chill out", "진정해, 느긋하게 해", "Chill out, we still have plenty of time.", "진정해, 아직 시간 많아."),
    ("I'm beat.", "완전 지쳤어.", "I'm beat. I think I'll go to bed early.", "완전 지쳤어. 일찍 자야겠다."),
    ("Piece of cake", "식은 죽 먹기", "Don't worry, the test was a piece of cake.", "걱정 마, 시험 엄청 쉬웠어."),
    ("Break a leg!", "행운을 빌어! (공연·발표 전)", "Big presentation today? Break a leg!", "오늘 큰 발표 있지? 잘해!"),
    ("Hit the road", "출발하다, 떠나다", "It's getting late. Let's hit the road.", "늦었다. 이제 출발하자."),
    ("Couch potato", "소파에서 TV만 보는 사람", "I was a total couch potato all weekend.", "주말 내내 소파에서 뒹굴었어."),
    ("Cool beans", "좋아, 알았어 (장난스럽게)", "You'll bring the snacks? Cool beans.", "네가 간식 가져온다고? 좋아."),
    ("Bummer", "아쉽다, 실망이다", "The concert got canceled? What a bummer.", "콘서트 취소됐어? 진짜 아쉽다."),
    ("It's a deal.", "좋아, 그렇게 하자.", "You cook, I'll do the dishes. It's a deal?", "네가 요리하고 내가 설거지. 콜?"),
    ("I'm all ears.", "잘 듣고 있어, 말해 봐.", "You have a plan? I'm all ears.", "계획 있어? 말해 봐, 듣고 있어."),
    ("Sleep on it", "하룻밤 생각해 보다", "You don't have to decide now. Sleep on it.", "지금 정할 필요 없어. 하룻밤 생각해 봐."),
    ("Gnarly", "(80년대) 끝내주는 / 끔찍한", "Dude, that wave was gnarly!", "야, 그 파도 끝내줬어!"),
    ("Talk to the hand", "(90년대) 말 걸지 마", "Talk to the hand, I'm not listening.", "됐어, 안 들을래."),
    ("As if!", "(90년대) 퍽이나! 말도 안 돼!", "Me, scared of a spider? As if!", "내가 거미를 무서워한다고? 퍽이나!"),
    ("Da bomb", "(90년대) 최고야", "This pizza is da bomb.", "이 피자 최고야."),
    ("Cut to the chase", "본론으로 들어가다", "We're short on time, so let's cut to the chase.", "시간 없으니까 본론만 말하자."),
    ("Under the weather", "몸이 좀 안 좋은", "I'm feeling a bit under the weather today.", "오늘 몸이 좀 안 좋아."),
    ("Rain check", "다음으로 미루기", "Can I take a rain check on dinner?", "저녁은 다음에 해도 될까?"),
    ("Speak of the devil", "호랑이도 제 말 하면 온다", "Speak of the devil! We were just talking about you.", "호랑이도 제 말 하면 온다더니! 네 얘기 하고 있었어."),
    ("Cost an arm and a leg", "엄청 비싸다", "That new phone costs an arm and a leg.", "그 새 폰 엄청 비싸."),
    ("Easy peasy", "아주 쉬운", "Making pancakes is easy peasy.", "팬케이크 만들기는 완전 쉬워."),
];

/// Recent slang, mostly from social media (2010s–2020s).
pub const RECENT: &[Expr] = &[
    ("No cap", "진짜야, 거짓말 아니야", "That was the best burger I've ever had, no cap.", "진짜 지금까지 먹은 버거 중 최고였어."),
    ("Low-key", "은근히, 살짝", "I'm low-key excited about the trip.", "여행 은근 기대돼."),
    ("High-key", "대놓고, 완전", "I high-key love this song.", "이 노래 완전 좋아."),
    ("It's giving…", "~한 느낌이야, ~ 분위기야", "That outfit? It's giving main character.", "그 옷? 완전 주인공 느낌이야."),
    ("Spill the tea", "소문·근황 털어놔", "You went on a date? Spill the tea!", "데이트했다고? 다 말해 봐!"),
    ("Ghost (someone)", "연락을 끊고 잠수 타다", "He ghosted me after our second date.", "두 번째 데이트 후에 걔가 잠수 탔어."),
    ("FOMO", "나만 놓칠까 봐 불안한 마음", "I have major FOMO seeing everyone's vacation photos.", "다들 휴가 사진 보니까 나만 뒤처지는 기분이야."),
    ("Rizz", "이성을 끄는 매력, 말발", "He's got so much rizz, everyone likes him.", "걔 매력 장난 아니야, 다들 좋아해."),
    ("Touch grass", "인터넷 그만하고 밖에 좀 나가", "You've been gaming for ten hours. Go touch grass.", "10시간째 게임 중이잖아. 밖에 좀 나가."),
    ("Hits different", "유난히 좋다, 느낌이 다르다", "Coffee on a rainy morning just hits different.", "비 오는 아침 커피는 느낌이 달라."),
    ("Slay", "끝내주게 잘하다", "You slayed that presentation!", "발표 완전 찢었어!"),
    ("Lit", "신나는, 끝내주는", "The party last night was lit.", "어젯밤 파티 완전 신났어."),
    ("Salty", "삐친, 짜증 난", "He's still salty about losing the game.", "걔 아직도 게임 진 거 때문에 삐져 있어."),
    ("Sus", "수상한", "He said he was sick, but he posted beach photos. Sus.", "아프다더니 해변 사진 올렸어. 수상해."),
    ("Mid", "그저 그런, 별로인", "Honestly, the movie was mid.", "솔직히 영화 그냥 그랬어."),
    ("Bet", "좋아, 알았어", "\"See you at seven?\" \"Bet.\"", "\"7시에 봐?\" \"콜.\""),
    ("Vibe check", "분위기·기분 확인", "Quick vibe check: is everyone okay?", "잠깐 분위기 체크: 다들 괜찮아?"),
    ("Main character energy", "주인공 같은 당당함", "Walking in with sunglasses? Main character energy.", "선글라스 끼고 등장? 완전 주인공 포스."),
    ("Delulu", "망상에 빠진 (delusional)", "She thinks he'll text back. She's a little delulu.", "걔가 답장할 거라고 믿다니, 좀 망상이야."),
    ("Situationship", "썸과 연애 사이의 애매한 관계", "We're not dating, it's more of a situationship.", "사귀는 건 아니고, 애매한 사이야."),
    ("Understood the assignment", "기대 이상으로 딱 맞게 해냈다", "Your costume is perfect. You understood the assignment.", "의상 완벽해. 제대로 이해했네."),
    ("Ate (and left no crumbs)", "완벽하게 해냈다", "Her performance? She ate and left no crumbs.", "걔 무대? 완전 찢었어."),
    ("Glow-up", "몰라보게 멋있어짐", "Have you seen her lately? What a glow-up!", "요즘 걔 봤어? 완전 달라졌어!"),
    ("Big mood", "완전 내 기분이야, 공감", "\"I just want to sleep all day.\" \"Big mood.\"", "\"하루 종일 자고 싶다.\" \"완전 공감.\""),
    ("Stan", "열성 팬이다, 덕질하다", "I stan this band so hard.", "나 이 밴드 완전 덕질해."),
];

/// One classic and one recent expression the learner hasn't saved yet.
pub fn pick_for_today(knows: impl Fn(&str) -> bool) -> Vec<Expr> {
    [CLASSIC, RECENT].iter().filter_map(|bank| bank.iter().find(|e| !knows(e.0)).copied()).collect()
}
