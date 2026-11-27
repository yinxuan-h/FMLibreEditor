// A loan arranged for the game to make on its next day: a player of one of the best known clubs, to the next one, until
// the end of the season, with 80% of his wage paid by the club he joins, a monthly fee and an option to buy. The game
// makes such a loan in a transfer window or out of one (see docs/recon/fm27-loans.md). The plan names the clubs, the next
// day, the loan's last day, the wage the club pays and the option; a last day past his contract, a share over 100% and
// an option of nothing are refused; arranging it logs a change of his club with that day and the loan's end, while he
// stays at his club until then (the probe reads him there, not on loan); a second deal for him is refused while the
// first stands; the undo takes it back. The game itself is not moved on: what it does on the day is checked in the game.
import { clubPlayers, topClubs } from "../lib/witness.mjs";

export const name = "lend";
export const writes = true;

/** The club the probe reads the player at. */
const probedClub = (probe, uid) => probe.run(["player", uid]).match(/^\s+club\s+(.*)$/m)?.[1]?.trim() ?? "?";

/** Whether the probe reads the player as on loan. */
const probedLoan = (probe, uid) => /^\s*\d+\s+.+->.+\d{4}-\d{2}-\d{2} to /m.test(probe.run(["loans", uid]));

/** The day after `date` ({year, month, day}), as a number like 20261019. */
function nextDay(date) {
  const next = new Date(Date.UTC(date.year, date.month - 1, date.day + 1));
  return next.getUTCFullYear() * 10000 + (next.getUTCMonth() + 1) * 100 + next.getUTCDate();
}

/** The day after `number` (a date as a number like 20280629), as such a number. */
function dayAfter(number) {
  return nextDay({ year: Math.floor(number / 10000), month: Math.floor(number / 100) % 100, day: number % 100 });
}

export async function run({ app, report, probe }) {
  const connection = await app.must("current_connection");
  if (!connection.arrangesLoans) {
    report.skip("arranges a loan for the game to make", "this build or system does not arrange loans");
    return;
  }
  const [from, to] = await topClubs(app, 2);
  const tomorrow = nextDay(connection.gameDate);
  // The first of the club's players the app agrees to arrange a loan for, past its best three.
  let found = null;
  const refusals = new Set();
  for (const player of (await clubPlayers(app, from, 20)).slice(3)) {
    const until = await app.must("season_loan_end", { uid: player.uid });
    if (until === null) {
      continue;
    }
    const loan = { until, wageShare: 80, monthlyFee: 25_000, optionFee: 30_000_000 };
    const plan = await app.invoke("plan_arranged_loan", { uid: player.uid, club: to.uid, loan });
    if (plan.ok) {
      found = { player, loan, plan: plan.value };
      break;
    }
    refusals.add(plan.error.code);
  }
  if (!found) {
    report.check("finds a player the app agrees to arrange a loan for", false, [...refusals].join(", "));
    return;
  }
  const { player, loan, plan } = found;
  report.note(`witness: ${player.name} (${player.uid}), ${from.name} → ${to.name} until ${loan.until}`);
  const ends = player.contractExpires;
  const ownEnd = ends ? ends.year * 10000 + ends.month * 100 + ends.day : 0;
  report.check(
    "the default last day is 29 June, or his contract's end when that comes first",
    loan.until > tomorrow && (loan.until % 10000 === 629 || loan.until === ownEnd),
    `${loan.until}, his contract ends ${ownEnd}`,
  );
  report.check(
    "the plan names the clubs, the next day and the loan's last day",
    plan.from.uid === from.uid && plan.to.uid === to.uid && plan.moveDay === tomorrow && plan.until === loan.until,
    `${plan.from.name} → ${plan.to.name}, on ${plan.moveDay} (next day ${tomorrow}), until ${plan.until}`,
  );
  const ownWage = (await app.must("read_values", { entity: "player", uid: player.uid }))["contract.wage"];
  report.check(
    "and the club he joins pays 80% of his wage, and the monthly fee",
    plan.wageShare === 80 &&
      plan.monthlyFee === 25_000 &&
      (typeof ownWage !== "number" || plan.wage === Math.floor((ownWage * 80) / 100)),
    `£${plan.wage} a week of £${ownWage}`,
  );
  report.check("and the option to buy", plan.optionFee === 30_000_000, `${plan.optionFee}`);
  const free = await app.invoke("plan_arranged_loan", { uid: player.uid, club: to.uid, loan: { ...loan, optionFee: 0 } });
  report.check(
    "an option to buy for nothing is refused",
    !free.ok && free.error.code === "move.outOfRange",
    free.ok ? "planned" : free.error.code,
  );
  const late = await app.invoke("plan_arranged_loan", {
    uid: player.uid,
    club: to.uid,
    loan: { ...loan, until: dayAfter(ownEnd) },
  });
  report.check(
    "a loan that ends after his contract is refused",
    !late.ok && late.error.code === "loan.end",
    late.ok ? "planned" : late.error.code,
  );
  const greedy = await app.invoke("plan_arranged_loan", { uid: player.uid, club: to.uid, loan: { ...loan, wageShare: 101 } });
  report.check(
    "and so is a share of his wage over 100%",
    !greedy.ok && greedy.error.code === "move.outOfRange",
    greedy.ok ? "planned" : greedy.error.code,
  );

  const arranged = await app.must("arrange_loan", { uid: player.uid, club: to.uid, loan });
  report.check(
    "arranges it, logged as a change of his club on that day, until the loan's last day",
    arranged.field === "club" &&
      arranged.moved?.from === from.name &&
      arranged.moved?.to === to.name &&
      arranged.moved?.arranged === tomorrow &&
      arranged.moved?.loanUntil === loan.until,
    JSON.stringify(arranged.moved),
  );
  const club = probedClub(probe, player.uid);
  report.check(
    "he is at his club, not on loan, until the day",
    club.startsWith(from.name.slice(0, 20)) && !probedLoan(probe, player.uid),
    club,
  );
  const again = await app.invoke("plan_arranged_loan", { uid: player.uid, club: to.uid, loan });
  report.check(
    "a second loan for him is refused while the first stands",
    !again.ok && again.error.code === "arrange.underWay",
    again.ok ? "planned" : again.error.code,
  );
  const transfer = await app.invoke("plan_arranged_transfer", { uid: player.uid, club: to.uid, terms: {}, fee: 0 });
  report.check(
    "and so is a transfer",
    !transfer.ok && transfer.error.code === "arrange.underWay",
    transfer.ok ? "planned" : transfer.error.code,
  );
  const log = await app.must("change_log");
  report.check(
    "the change log lists it",
    log.some((change) => change.id === arranged.id && change.moved?.loanUntil === loan.until),
  );
  await app.must("undo_change", { id: arranged.id });
  const after = await app.invoke("plan_arranged_loan", { uid: player.uid, club: to.uid, loan });
  report.check("the undo takes it back: he can be planned again", after.ok, after.ok ? "" : after.error.code);
  report.check("and he is still at his club", probedClub(probe, player.uid).startsWith(from.name.slice(0, 20)));
}
