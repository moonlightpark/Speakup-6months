//! Reading library: graded stories from picture-book fables to classic novels.
//!
//! Short fables are built in (retold for learners). Longer books are public-domain originals
//! from Project Gutenberg, downloaded once on request and cached in the data folder.
//! Books are split into pages of paragraphs so a session is a comfortable read.

use anyhow::{Result, anyhow};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Source {
    Builtin(&'static str),
    /// Project Gutenberg ebook number.
    Gutenberg(u32),
}

#[derive(Clone, Copy, Debug)]
pub struct Book {
    pub id: &'static str,
    pub title: &'static str,
    pub title_ko: &'static str,
    pub author: &'static str,
    /// Suitable learner levels (1..=10, see `cefr_of`).
    pub min_level: f32,
    pub max_level: f32,
    pub kind: &'static str,
    pub blurb_ko: &'static str,
    pub source: Source,
}

pub const BOOKS: &[Book] = &[
    Book { id: "lion-mouse", title: "The Lion and the Mouse", title_ko: "사자와 생쥐", author: "Aesop (retold)", min_level: 1.0, max_level: 3.0, kind: "동화", blurb_ko: "작은 친구도 큰 도움이 될 수 있어요.", source: Source::Builtin(LION_MOUSE) },
    Book { id: "boy-wolf", title: "The Boy Who Cried Wolf", title_ko: "양치기 소년", author: "Aesop (retold)", min_level: 1.0, max_level: 3.0, kind: "동화", blurb_ko: "거짓말을 하면 아무도 믿지 않아요.", source: Source::Builtin(BOY_WOLF) },
    Book { id: "tortoise-hare", title: "The Tortoise and the Hare", title_ko: "토끼와 거북이", author: "Aesop (retold)", min_level: 1.5, max_level: 3.5, kind: "동화", blurb_ko: "천천히 꾸준히 가면 이겨요.", source: Source::Builtin(TORTOISE_HARE) },
    Book { id: "ant-grasshopper", title: "The Ant and the Grasshopper", title_ko: "개미와 베짱이", author: "Aesop (retold)", min_level: 1.5, max_level: 3.5, kind: "동화", blurb_ko: "미리 준비하는 지혜.", source: Source::Builtin(ANT_GRASSHOPPER) },
    Book { id: "red-hen", title: "The Little Red Hen", title_ko: "빨간 암탉", author: "Folk tale (retold)", min_level: 2.0, max_level: 4.0, kind: "동화", blurb_ko: "일한 사람이 빵을 먹어요.", source: Source::Builtin(RED_HEN) },
    Book { id: "goldilocks", title: "Goldilocks and the Three Bears", title_ko: "골디락스와 곰 세 마리", author: "Folk tale (retold)", min_level: 2.0, max_level: 4.5, kind: "동화", blurb_ko: "너무 뜨겁지도, 차갑지도 않게.", source: Source::Builtin(GOLDILOCKS) },
    Book { id: "ugly-duckling", title: "The Ugly Duckling", title_ko: "미운 오리 새끼", author: "H. C. Andersen (retold)", min_level: 3.0, max_level: 5.0, kind: "동화", blurb_ko: "나다움을 찾아가는 이야기.", source: Source::Builtin(UGLY_DUCKLING) },
    Book { id: "peter-rabbit", title: "The Tale of Peter Rabbit", title_ko: "피터 래빗 이야기", author: "Beatrix Potter", min_level: 3.5, max_level: 6.0, kind: "어린이 고전", blurb_ko: "말썽꾸러기 토끼의 모험 (원문).", source: Source::Gutenberg(14838) },
    Book { id: "oz", title: "The Wonderful Wizard of Oz", title_ko: "오즈의 마법사", author: "L. Frank Baum", min_level: 4.5, max_level: 7.0, kind: "어린이 고전", blurb_ko: "도로시와 친구들의 여행.", source: Source::Gutenberg(55) },
    Book { id: "alice", title: "Alice's Adventures in Wonderland", title_ko: "이상한 나라의 앨리스", author: "Lewis Carroll", min_level: 5.0, max_level: 8.0, kind: "어린이 고전", blurb_ko: "토끼 굴 속 이상한 세계.", source: Source::Gutenberg(11) },
    Book { id: "secret-garden", title: "The Secret Garden", title_ko: "비밀의 화원", author: "Frances Hodgson Burnett", min_level: 5.5, max_level: 8.0, kind: "소설", blurb_ko: "닫힌 정원과 마음이 열리는 이야기.", source: Source::Gutenberg(113) },
    Book { id: "tom-sawyer", title: "The Adventures of Tom Sawyer", title_ko: "톰 소여의 모험", author: "Mark Twain", min_level: 6.0, max_level: 9.0, kind: "소설", blurb_ko: "미시시피 강가 소년의 장난과 모험.", source: Source::Gutenberg(74) },
    Book { id: "christmas-carol", title: "A Christmas Carol", title_ko: "크리스마스 캐럴", author: "Charles Dickens", min_level: 6.5, max_level: 9.0, kind: "소설", blurb_ko: "구두쇠 스크루지의 하룻밤.", source: Source::Gutenberg(46) },
    Book { id: "sherlock", title: "The Adventures of Sherlock Holmes", title_ko: "셜록 홈즈의 모험", author: "Arthur Conan Doyle", min_level: 7.0, max_level: 10.0, kind: "소설", blurb_ko: "명탐정의 추리 단편집.", source: Source::Gutenberg(1661) },
    Book { id: "gatsby", title: "The Great Gatsby", title_ko: "위대한 개츠비", author: "F. Scott Fitzgerald", min_level: 8.0, max_level: 10.0, kind: "소설", blurb_ko: "재즈 시대의 꿈과 사랑.", source: Source::Gutenberg(64317) },
    Book { id: "pride", title: "Pride and Prejudice", title_ko: "오만과 편견", author: "Jane Austen", min_level: 8.5, max_level: 10.0, kind: "소설", blurb_ko: "엘리자베스와 다아시의 이야기.", source: Source::Gutenberg(1342) },
];

pub fn book(id: &str) -> Option<&'static Book> {
    BOOKS.iter().find(|b| b.id == id)
}

/// Books that fit the learner now (a little easier to a little harder), easiest first.
pub fn recommended(level: f32) -> Vec<&'static Book> {
    BOOKS.iter().filter(|b| level + 0.5 >= b.min_level && level - 1.0 <= b.max_level).collect()
}

/// A readable unit: a title and its paragraphs.
#[derive(Clone, Debug, PartialEq)]
pub struct Page {
    pub heading: String,
    pub paragraphs: Vec<String>,
}

/// Paragraphs per page for long books.
const PAGE_PARAGRAPHS: usize = 8;

pub fn cache_path(dir: &Path, id: u32) -> PathBuf {
    dir.join(format!("pg{id}.txt"))
}

pub fn is_downloaded(dir: &Path, b: &Book) -> bool {
    match b.source {
        Source::Builtin(_) => true,
        Source::Gutenberg(id) => cache_path(dir, id).exists(),
    }
}

/// Downloads a Gutenberg book once (plain-text UTF-8 edition).
pub fn download(dir: &Path, id: u32) -> Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let dest = cache_path(dir, id);
    if dest.exists() {
        return Ok(dest);
    }
    let client = reqwest::blocking::Client::builder().timeout(std::time::Duration::from_secs(120)).user_agent("SpeakUp6M/1.0 (personal reading)").build()?;
    let url = format!("https://www.gutenberg.org/cache/epub/{id}/pg{id}.txt");
    let text = client.get(&url).send()?.error_for_status()?.text()?;
    if text.len() < 2000 {
        return Err(anyhow!("책을 받지 못했습니다 ({url})"));
    }
    let tmp = dest.with_extension("part");
    std::fs::write(&tmp, &text)?;
    std::fs::rename(&tmp, &dest)?;
    Ok(dest)
}

/// Loads a book as pages (built-in text, or a downloaded Gutenberg file).
pub fn load_pages(dir: &Path, b: &Book) -> Result<Vec<Page>> {
    match b.source {
        Source::Builtin(text) => Ok(vec![Page { heading: b.title.to_string(), paragraphs: paragraphs(text) }]),
        Source::Gutenberg(id) => {
            let raw = std::fs::read_to_string(cache_path(dir, id)).map_err(|_| anyhow!("책을 먼저 받아 주세요."))?;
            let mut pages = paginate(&strip_gutenberg(&raw));
            for p in pages.iter_mut().filter(|p| p.heading.is_empty() || p.heading.starts_with(" (")) {
                p.heading = format!("{}{}", b.title, p.heading);
            }
            Ok(pages)
        }
    }
}

/// Text between the "*** START OF" and "*** END OF" markers (drops the license boilerplate).
pub fn strip_gutenberg(raw: &str) -> String {
    let raw = raw.replace("\r\n", "\n");
    let start = raw.find("*** START OF").and_then(|i| raw[i..].find('\n').map(|j| i + j + 1)).unwrap_or(0);
    let end = raw.find("*** END OF").unwrap_or(raw.len());
    raw[start..end.max(start)].to_string()
}

/// Blank-line separated paragraphs with line breaks joined.
pub fn paragraphs(text: &str) -> Vec<String> {
    text.replace("\r\n", "\n")
        .split("\n\n")
        .map(|p| p.split_whitespace().collect::<Vec<_>>().join(" "))
        .map(|p| p.replace('_', ""))
        .filter(|p| !p.is_empty())
        .collect()
}

fn is_roman(w: &str) -> bool {
    !w.is_empty() && w.chars().all(|c| "IVXLC".contains(c))
}

/// "CHAPTER I.", "Chapter 12", "STAVE ONE.", "I. A SCANDAL IN BOHEMIA", "IV" — a real chapter start.
fn is_chapter_heading(p: &str) -> bool {
    let words: Vec<&str> = p.split_whitespace().collect();
    if words.is_empty() || words.len() > 12 {
        return false;
    }
    let first = words[0].trim_end_matches(['.', ':']);
    let up = first.to_uppercase();
    if matches!(up.as_str(), "CHAPTER" | "STAVE" | "BOOK" | "PART") {
        return words.len() >= 2;
    }
    // Roman numeral alone ("IV") or followed by a title ("I. A Scandal in Bohemia").
    is_roman(first) && (words.len() == 1 || words[0].ends_with('.'))
}

fn word_count(p: &str) -> usize {
    p.split_whitespace().count()
}

/// Looks like story text rather than a title page, list of illustrations or publisher note.
fn is_prose(p: &str) -> bool {
    let low = p.to_lowercase();
    word_count(p) >= 8
        && p.chars().any(|c| c.is_lowercase())
        && !["printed", "published", "copyright", "edition", "illustrated by", "contents"].iter().any(|k| low.contains(k))
}

/// Splits a book into chapters (by headings), then into pages of a few paragraphs.
pub fn paginate(text: &str) -> Vec<Page> {
    let paras: Vec<String> = paragraphs(text).into_iter().filter(|p| !(p.starts_with("[Illustration") && p.ends_with(']'))).collect();
    let headings: Vec<usize> = (0..paras.len()).filter(|&i| is_chapter_heading(&paras[i])).collect();
    // The story starts at the first chapter heading followed by real prose (a table of
    // contents has headings right after each other); books without chapters start at the
    // first run of three sentence-like paragraphs.
    let start = headings
        .iter()
        .find(|&&h| {
            let next = headings.iter().find(|&&x| x > h).copied().unwrap_or(paras.len());
            paras[h + 1..next].iter().filter(|p| is_prose(p)).map(|p| word_count(p)).sum::<usize>() >= 200
        })
        .copied()
        .unwrap_or_else(|| {
            (0..paras.len()).find(|&i| is_prose(&paras[i]) && paras[i..].iter().take(3).filter(|p| is_prose(p)).count() >= 2).unwrap_or(0)
        });
    let mut chapters: Vec<(String, Vec<String>)> = Vec::new();
    for p in &paras[start..] {
        // A short line right after "CHAPTER I." is the chapter's title ("Down the Rabbit-Hole").
        let subtitle = chapters.last().is_some_and(|(h, body)| !h.is_empty() && body.is_empty())
            && word_count(p) <= 10
            && !p.ends_with(['.', '"', '”', '!', '?', ',']);
        if is_chapter_heading(p) || subtitle {
            match chapters.last_mut() {
                Some((h, body)) if body.is_empty() => {
                    if !h.is_empty() {
                        h.push_str(" — ");
                    }
                    h.push_str(p.trim_matches(['[', ']']));
                }
                _ => chapters.push((p.trim_matches(['[', ']']).to_string(), Vec::new())),
            }
        } else {
            if chapters.is_empty() {
                chapters.push((String::new(), Vec::new()));
            }
            chapters.last_mut().unwrap().1.push(p.clone());
        }
    }
    let mut pages = Vec::new();
    for (heading, body) in chapters.into_iter().filter(|c| !c.1.is_empty()) {
        let n = body.len().div_ceil(PAGE_PARAGRAPHS);
        for (i, chunk) in body.chunks(PAGE_PARAGRAPHS).enumerate() {
            let h = if n > 1 { format!("{heading} ({}/{n})", i + 1) } else { heading.clone() };
            pages.push(Page { heading: h, paragraphs: chunk.to_vec() });
        }
    }
    pages
}

// ---------------------------------------------------------------- built-in stories

const LION_MOUSE: &str = "A big lion was sleeping under a tree. A little mouse ran over his nose. The lion woke up and caught the mouse with his big paw.

\"Please don't eat me!\" said the mouse. \"Let me go, and one day I will help you.\"

The lion laughed. \"You are so small. How can you help me?\" But he was not hungry, so he let the mouse go.

A few days later, the lion walked into a hunter's net. He pulled and pulled, but he couldn't get out. He roared and roared.

The little mouse heard him. She ran to the lion and started to bite the net. Bite, bite, bite. Soon there was a big hole, and the lion was free.

\"Thank you, my little friend,\" said the lion. \"You were right. Even a small friend can be a big help.\"";

const BOY_WOLF: &str = "There was once a boy who watched the sheep on a hill near his village. Every day was the same, and he was bored.

One morning he had an idea. He ran down the hill and shouted, \"Wolf! Wolf! A wolf is eating the sheep!\" The villagers dropped their work and ran up the hill to help. But there was no wolf. The boy laughed and laughed.

A few days later, he did it again. \"Wolf! Wolf!\" Again the villagers ran up the hill, and again there was no wolf. They were very angry. \"Don't lie to us,\" they said.

Then one evening, a real wolf came out of the forest. The boy was scared. \"Wolf! Wolf! Please help me!\" he cried. But nobody came. They thought he was lying again.

The wolf chased the sheep away, and the boy sat alone on the hill. That night he learned something important: if you tell lies, people won't believe you, even when you tell the truth.";

const TORTOISE_HARE: &str = "A hare loved to tell everyone how fast he was. \"Nobody can beat me,\" he said. \"I'm the fastest animal in the forest.\"

A tortoise was tired of listening to him. \"Okay,\" she said slowly. \"Let's have a race.\"

The hare laughed so hard he almost fell over. \"You? Race me? Sure, why not!\"

The next morning, all the animals came to watch. On \"Go!\", the hare shot off like the wind. The tortoise started walking, one slow step at a time.

Soon the hare was so far ahead that he couldn't even see the tortoise. \"This is too easy,\" he thought. \"I'll take a little nap under this tree.\" And he fell fast asleep.

The tortoise kept going. Step by step, she walked past the sleeping hare. She didn't stop, and she didn't give up.

When the hare finally woke up, the sun was going down. He ran as fast as he could, but it was too late. The tortoise was already crossing the finish line, and all the animals were cheering.

Slow and steady wins the race.";

const ANT_GRASSHOPPER: &str = "It was a hot summer day. A grasshopper was singing and dancing in the field. An ant walked by, carrying a heavy piece of corn.

\"Why are you working so hard?\" asked the grasshopper. \"Come and play with me!\"

\"I'm saving food for the winter,\" said the ant. \"You should do the same.\"

\"Winter? That's months away,\" laughed the grasshopper. \"We have plenty of food right now.\" And he went back to his song.

All summer long, the ant worked and the grasshopper played. Then the cold winds came, and snow covered the field. The grasshopper couldn't find anything to eat. He was cold and very hungry.

He knocked on the ant's door. \"Please, could you share some food with me?\"

The ant felt sorry for him and gave him a little corn. \"Next year,\" she said, \"work a little in the summer, and you won't be hungry in the winter.\"

It's smart to get ready today for what you will need tomorrow.";

const RED_HEN: &str = "Once upon a time, a little red hen lived on a farm with a lazy cat, a sleepy dog and a noisy duck.

One day the hen found some seeds of wheat. \"Who will help me plant these seeds?\" she asked.

\"Not me,\" said the cat. \"Not me,\" said the dog. \"Not me,\" said the duck.

\"Then I'll do it myself,\" said the little red hen. And she did.

The wheat grew tall and golden. \"Who will help me cut the wheat?\" she asked. \"Not me,\" said the cat, the dog and the duck. So she cut it herself.

\"Who will help me take the wheat to the mill?\" Nobody helped. \"Who will help me bake the bread?\" Nobody helped again. So the little red hen did all the work by herself.

Soon the warm, delicious smell of fresh bread filled the farm. \"Who will help me eat this bread?\" asked the hen.

\"Me!\" said the cat. \"Me!\" said the dog. \"Me!\" said the duck.

\"Oh no,\" said the little red hen. \"You didn't help me plant, cut, carry or bake. So my chicks and I will eat it ourselves.\" And that's exactly what they did.";

const GOLDILOCKS: &str = "Once upon a time, three bears lived in a little house in the woods: Papa Bear, Mama Bear and Baby Bear.

One morning they made porridge for breakfast, but it was too hot to eat. \"Let's go for a walk while it cools down,\" said Mama Bear.

While they were out, a girl named Goldilocks found their house. The door was open, so she walked right in. On the table were three bowls of porridge.

She tasted the big bowl. \"Too hot!\" She tasted the middle bowl. \"Too cold!\" Then she tasted the little bowl. \"Just right!\" And she ate it all up.

Next she saw three chairs. The big chair was too hard. The middle chair was too soft. The little chair was just right, but when she sat on it, it broke into pieces!

Feeling tired, she went upstairs and found three beds. The big bed was too hard, the middle bed was too soft, but the little bed was just right. She lay down and fell asleep.

Soon the bears came home. \"Someone's been eating my porridge,\" said Papa Bear. \"Someone's been sitting in my chair,\" said Mama Bear. \"Someone's broken my chair!\" cried Baby Bear.

They went upstairs. \"Someone's sleeping in my bed,\" said Baby Bear, \"and she's still here!\"

Goldilocks woke up, saw three bears looking at her, and screamed. She jumped out of the bed, ran down the stairs and out of the house, and she never came back again.";

const UGLY_DUCKLING: &str = "On a quiet farm, a mother duck was waiting for her eggs to hatch. One by one, the little yellow ducklings came out. But the last egg was bigger than the others, and the bird that came out of it was large, gray and clumsy.

\"What an ugly duckling,\" said the other animals. The hens pecked at him, the ducks laughed at him, and even his brothers and sisters didn't want to play with him.

The ugly duckling felt so sad and lonely that he ran away. He went from place to place, but everywhere he went, people and animals pushed him away. \"You don't belong here,\" they said.

Winter came, and it was long and hard. The duckling hid among the reeds by a frozen lake, cold and hungry. Many times he thought he would not live to see the spring.

But spring did come. The sun grew warm, and the duckling felt that his wings were strong. He flew to a beautiful lake where three white swans were swimming.

\"They'll chase me away too,\" he thought. Still, he swam toward them and bowed his head. Then he saw his reflection in the water. He wasn't a gray, clumsy duckling anymore. He was a swan, white and elegant, just like them.

The swans welcomed him, and the children by the lake shouted, \"Look, there's a new swan! He's the most beautiful one of all!\"

He didn't feel proud, only happy. He had been treated badly for being different, but now he knew who he really was.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_books_have_paragraphs() {
        for b in BOOKS.iter().filter(|b| matches!(b.source, Source::Builtin(_))) {
            let pages = load_pages(Path::new("."), b).unwrap();
            assert!(pages[0].paragraphs.len() >= 5, "{}", b.id);
        }
    }

    #[test]
    fn gutenberg_text_is_split_into_chapters() {
        let raw = "Header\n*** START OF THE PROJECT GUTENBERG EBOOK X ***\n\nContents\n\nCHAPTER I.\n\nCHAPTER II.\n\n\
                   CHAPTER I.\n\nDown the Rabbit-Hole\n\n"
            .to_string()
            + &"Alice was beginning to get very tired of sitting by her sister on the bank, and of having nothing to do: once or twice she had peeped into the book.\n\n".repeat(10)
            + "CHAPTER II.\n\nThe Pool of Tears\n\n"
            + &"Curiouser and curiouser! cried Alice, she was so much surprised that for the moment she quite forgot how to speak good English and more words.\n\n".repeat(3)
            + "*** END OF THE PROJECT GUTENBERG EBOOK X ***\nLicense";
        let pages = paginate(&strip_gutenberg(&raw));
        assert_eq!(pages.len(), 3, "{pages:#?}");
        assert_eq!(pages[0].heading, "CHAPTER I. — Down the Rabbit-Hole (1/2)");
        assert_eq!(pages[0].paragraphs.len(), 8);
        assert_eq!(pages[2].heading, "CHAPTER II. — The Pool of Tears");
        assert!(!pages.iter().any(|p| p.paragraphs.iter().any(|x| x.contains("License"))));
    }

    /// Downloads real books and prints how they were paginated.
    /// SPEAKUP_TEST_LIBRARY=<dir> cargo test real_books -- --ignored --nocapture
    #[test]
    #[ignore]
    fn real_books_paginate() {
        let dir = PathBuf::from(std::env::var("SPEAKUP_TEST_LIBRARY").expect("SPEAKUP_TEST_LIBRARY"));
        for b in BOOKS.iter().filter(|b| matches!(b.source, Source::Gutenberg(_))) {
            if let Source::Gutenberg(id) = b.source {
                download(&dir, id).unwrap();
            }
            let pages = load_pages(&dir, b).unwrap();
            println!("{}: {} pages | first: {:?} | {:.80}", b.id, pages.len(), pages[0].heading, pages[0].paragraphs[0]);
            assert!(pages.len() >= 2, "{}", b.id);
            assert!(pages.iter().all(|p| !p.paragraphs.iter().any(|x| x.contains("Project Gutenberg License"))));
        }
    }

    #[test]
    fn recommendations_follow_level() {
        assert!(recommended(1.5).iter().all(|b| b.kind == "동화"));
        assert!(recommended(9.0).iter().any(|b| b.id == "pride"));
        assert!(!recommended(2.0).iter().any(|b| b.id == "pride"));
    }
}
