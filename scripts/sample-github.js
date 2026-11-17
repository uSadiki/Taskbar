// Development check: an expression for `cdp.mjs eval dock` that puts sample
// GitHub data into the dock's store (dev builds only), so the GitHub tile and
// popover can be looked at without a real token. The next real refresh
// replaces it.
(() => {
  const days = [];
  const start = new Date();
  start.setDate(start.getDate() - 370);
  for (let i = 0; i <= 370; i++) {
    const d = new Date(start);
    d.setDate(start.getDate() + i);
    const weekend = d.getDay() === 0 || d.getDay() === 6;
    const count = Math.max(0, Math.round((Math.sin(i / 9) + 1) * (weekend ? 1.5 : 5) - 2 + ((i * 7) % 5)));
    days.push({
      date: `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`,
      count,
      level: count === 0 ? 0 : count < 3 ? 1 : count < 6 ? 2 : count < 9 ? 3 : 4,
    });
  }
  const now = new Date().toISOString();
  const pr = (n, title, draft = false) => ({
    title,
    url: `https://github.com/example/dock/pull/${n}`,
    number: n,
    repo: "example/dock",
    draft,
    updatedAt: now,
  });
  window.__online.setState({
    github: {
      status: "ok",
      message: null,
      data: {
        login: "sample-user",
        totalYear: days.reduce((a, d) => a + d.count, 0),
        days,
        today: days.at(-1).count,
        currentStreak: 12,
        longestStreak: 41,
        authored: [pr(42, "Spring the glass when the dock changes edge"), pr(39, "Tidy the settings store", true)],
        authoredTotal: 2,
        reviewRequests: [pr(44, "Add a Hourly weather strip")],
        reviewTotal: 1,
        events: [
          { kind: "push", summary: "Pushed 3 commits to main", repo: "example/dock", url: "https://github.com/example/dock", createdAt: now },
          { kind: "pr", summary: "Opened pull request #42", repo: "example/dock", url: "https://github.com/example/dock/pull/42", createdAt: now },
          { kind: "review", summary: "Reviewed pull request #40", repo: "example/dock", url: "https://github.com/example/dock/pull/40", createdAt: now },
        ],
        fetchedAt: Math.floor(Date.now() / 1000),
      },
    },
  });
  return "sample GitHub data loaded";
})()
