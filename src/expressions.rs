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

/// High-frequency spoken chunks (sentence frames) — learned whole, they make speaking fluent
/// much faster than single words. No "…": the example sentence shows how each one continues.
pub const CHUNKS: &[Expr] = &[
    ("I was wondering if", "~해 주실 수 있을까 해서요 (정중한 부탁)", "I was wondering if you could help me with this.", "이것 좀 도와주실 수 있을까 해서요."),
    ("It depends on", "~에 따라 달라요", "It depends on the weather.", "날씨에 따라 달라요."),
    ("I'm thinking about", "~할까 생각 중이에요", "I'm thinking about changing jobs.", "이직할까 생각 중이에요."),
    ("Do you mind if I", "제가 ~해도 될까요?", "Do you mind if I open the window?", "창문 좀 열어도 될까요?"),
    ("I'm not sure if", "~인지 잘 모르겠어요", "I'm not sure if he's coming.", "그가 올지 잘 모르겠어요."),
    ("How about", "~하는 건 어때요?", "How about grabbing lunch together?", "같이 점심 먹는 건 어때요?"),
    ("I'm looking forward to", "~를 기대하고 있어요", "I'm looking forward to the weekend.", "주말이 기다려져요."),
    ("I used to", "예전에는 ~했었어요", "I used to live in Busan.", "예전에 부산에 살았어요."),
    ("I'm about to", "막 ~하려던 참이에요", "I'm about to leave the office.", "막 퇴근하려던 참이에요."),
    ("Would you like to", "~하실래요?", "Would you like to join us for dinner?", "저녁 같이 하실래요?"),
    ("I feel like", "~하고 싶어요 / ~인 것 같아요", "I feel like having pizza tonight.", "오늘 밤엔 피자가 당겨요."),
    ("The thing is", "문제는요, 실은요", "The thing is, I don't have enough time.", "문제는 시간이 부족하다는 거예요."),
    ("What I mean is", "제 말은요", "What I mean is, we need more people.", "제 말은 사람이 더 필요하다는 거예요."),
    ("It's not that", "~라는 게 아니라", "It's not that I don't like it. It's just expensive.", "싫다는 게 아니라 그냥 비싸서요."),
    ("I'm used to", "~에 익숙해요", "I'm used to waking up early.", "일찍 일어나는 게 익숙해요."),
    ("I can't wait to", "빨리 ~하고 싶어요", "I can't wait to see you.", "빨리 보고 싶어요."),
    ("Let me know if", "~하면 알려 주세요", "Let me know if you need anything.", "필요한 거 있으면 알려 주세요."),
    ("Is it okay if", "~해도 괜찮을까요?", "Is it okay if I leave a little early?", "조금 일찍 가도 괜찮을까요?"),
    ("I'd rather", "차라리 ~할래요", "I'd rather stay home tonight.", "오늘 밤은 차라리 집에 있을래요."),
    ("You should", "~하는 게 좋겠어요", "You should try the pasta here.", "여기 파스타 꼭 먹어 봐요."),
    ("I'm trying to", "~하려고 노력 중이에요", "I'm trying to eat healthier.", "건강하게 먹으려고 노력 중이에요."),
    ("It's hard to", "~하기 어려워요", "It's hard to say no to my boss.", "상사한테 거절하기 어려워요."),
    ("Thanks for", "~해 줘서 고마워요", "Thanks for waiting.", "기다려 줘서 고마워요."),
    ("Sorry for", "~해서 미안해요", "Sorry for the late reply.", "답장이 늦어서 미안해요."),
    ("What do you think about", "~에 대해 어떻게 생각해요?", "What do you think about the new plan?", "새 계획 어떻게 생각해요?"),
    ("Have you ever", "~해 본 적 있어요?", "Have you ever been to Japan?", "일본에 가 본 적 있어요?"),
    ("I've been meaning to", "진작 ~하려고 했어요", "I've been meaning to call you.", "진작 전화하려고 했어요."),
    ("It turns out", "알고 보니", "It turns out the store was closed.", "알고 보니 가게가 문을 닫았더라고요."),
    ("As far as I know", "제가 알기로는", "As far as I know, the meeting is at three.", "제가 알기로는 회의가 3시예요."),
    ("To be honest", "솔직히 말하면", "To be honest, I didn't like the movie.", "솔직히 그 영화 별로였어요."),
    ("I ended up", "결국 ~하게 됐어요", "I ended up staying until midnight.", "결국 자정까지 있게 됐어요."),
    ("There's no way", "~할 리가 없어요", "There's no way we can finish by Friday.", "금요일까지 끝낼 수 있을 리가 없어요."),
    ("I'm in the middle of", "~하는 중이에요", "I'm in the middle of a meeting.", "지금 회의 중이에요."),
    ("Make sure you", "꼭 ~하세요", "Make sure you lock the door.", "문 꼭 잠그세요."),
    ("Would it be possible to", "~하는 게 가능할까요?", "Would it be possible to change my seat?", "자리를 바꿀 수 있을까요?"),
    ("I just wanted to", "그냥 ~하고 싶었어요", "I just wanted to say thank you.", "그냥 고맙다고 말하고 싶었어요."),
    ("It's worth", "~할 가치가 있어요", "It's worth the wait.", "기다릴 만해요."),
    ("No matter what", "무슨 일이 있어도", "No matter what happens, I'll be there.", "무슨 일이 있어도 갈게요."),
    ("I'm supposed to", "~하기로 되어 있어요", "I'm supposed to meet him at six.", "6시에 그를 만나기로 했어요."),
    ("What if", "~하면 어쩌죠? / ~하면 어때요?", "What if it rains tomorrow?", "내일 비 오면 어쩌죠?"),
];

/// Today's expression: one a day keeps idioms and slang at about 5% of new material.
/// Classic expressions and recent slang take turns (odd days: recent slang).
pub fn pick_for_today(day: i64, knows: impl Fn(&str) -> bool) -> Vec<Expr> {
    let banks = if day % 2 == 1 { [RECENT, CLASSIC] } else { [CLASSIC, RECENT] };
    banks.iter().flat_map(|bank| bank.iter()).find(|e| !knows(e.0)).copied().into_iter().collect()
}

/// The next `n` high-frequency chunks the learner hasn't saved yet.
pub fn pick_chunks(n: usize, knows: impl Fn(&str) -> bool) -> Vec<Expr> {
    CHUNKS.iter().filter(|e| !knows(e.0)).take(n).copied().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_expression_a_day_alternating() {
        let none = |_: &str| false;
        assert_eq!(pick_for_today(2, none), vec![CLASSIC[0]]);
        assert_eq!(pick_for_today(3, none), vec![RECENT[0]]);
        // Falls back to the other bank once one is used up.
        let knows_recent = |t: &str| RECENT.iter().any(|e| e.0 == t);
        assert_eq!(pick_for_today(3, knows_recent), vec![CLASSIC[0]]);
        assert_eq!(pick_chunks(4, |t| t == CHUNKS[0].0)[0], CHUNKS[1]);
        assert!(CHUNKS.iter().all(|c| !c.0.contains('…') && c.2.contains(c.0)), "examples use their chunk");
    }
}
