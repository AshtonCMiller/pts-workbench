// v-reveal: the element slides up into place the first time it comes on screen. Ones that arrive together go a
// little after each other. (The animation is in styles.css: .reveal / .reveal.in)
let io = null;

function observer() {
  if (io) return io;
  io = new IntersectionObserver(
    (entries) => {
      let n = 0;
      for (const e of entries) {
        if (!e.isIntersecting) continue;
        e.target.style.setProperty('--reveal-delay', `${n++ * 70}ms`);
        e.target.classList.add('in');
        io.unobserve(e.target);
      }
    },
    { threshold: 0.12, rootMargin: '0px 0px -40px 0px' },
  );
  return io;
}

export const reveal = {
  mounted(el) {
    el.classList.add('reveal');
    if (typeof IntersectionObserver === 'undefined') el.classList.add('in');
    else observer().observe(el);
  },
  unmounted(el) {
    io?.unobserve(el);
  },
};
