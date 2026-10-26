// 스크롤에 따라 내비게이션 배경을 바꾸고, 섹션을 차례로 보여 주고, 최신 버전 번호를 채웁니다.
document.documentElement.classList.add("js");

const nav = document.getElementById("nav");
const hero = document.querySelector(".hero");
function onScroll() {
  const y = window.scrollY;
  const pastHero = y > hero.offsetHeight - 80;
  nav.classList.toggle("is-solid", pastHero);
  nav.classList.toggle("is-scrolled", y > 10 && !pastHero);
}
window.addEventListener("scroll", onScroll, { passive: true });
onScroll();

const io = new IntersectionObserver(
  (entries) => {
    for (const e of entries) {
      if (e.isIntersecting) {
        e.target.classList.add("is-in");
        io.unobserve(e.target);
      }
    }
  },
  { rootMargin: "0px 0px -8% 0px" }
);
document.querySelectorAll(".reveal").forEach((el, i) => {
  el.style.transitionDelay = `${(i % 5) * 60}ms`;
  io.observe(el);
});

// GitHub 릴리스에서 영어판(v*)과 일본어판(ja-v*) 최신 버전을 찾습니다. 실패하면 기본 문구를 그대로 둡니다.
fetch("https://api.github.com/repos/moonlightpark/Speakup-6months/releases?per_page=40")
  .then((r) => (r.ok ? r.json() : []))
  .then((list) => {
    const tags = list.filter((r) => !r.draft && !r.prerelease).map((r) => r.tag_name);
    const en = tags.find((t) => /^v\d/.test(t));
    const ja = tags.find((t) => /^ja-v\d/.test(t));
    document.querySelectorAll('[data-ver="en"]').forEach((el) => en && (el.textContent = en));
    document.querySelectorAll('[data-ver="ja"]').forEach((el) => ja && (el.textContent = ja.replace("ja-", "")));
  })
  .catch(() => {});

// 영어판/일본어판 화면 탭 (히어로, 문법): 탭 묶음마다 바로 옆 화면을 바꿉니다
document.querySelectorAll(".shot-tabs").forEach((tabs) => {
  const shot = tabs.parentElement.querySelector(".window img");
  tabs.querySelectorAll("button").forEach((b) =>
    b.addEventListener("click", () => {
      tabs.querySelectorAll("button").forEach((o) => o.setAttribute("aria-selected", String(o === b)));
      shot.src = b.dataset.shot;
      shot.alt = b.dataset.alt;
    })
  );
});
