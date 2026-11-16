import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { IconCalendar, IconChevron } from "./icons";
import "./date-range.css";

/** 日期一律用本地日期的 YYYY-MM-DD，与后端 get_usage 的 start / end 相同。 */
export function isoDay(date: Date) {
  const y = date.getFullYear();
  const m = String(date.getMonth() + 1).padStart(2, "0");
  const d = String(date.getDate()).padStart(2, "0");
  return `${y}-${m}-${d}`;
}

export function parseDay(value: string) {
  const [y, m, d] = value.split("-").map(Number);
  return new Date(y, (m ?? 1) - 1, d ?? 1);
}

function addDays(value: string, days: number) {
  const date = parseDay(value);
  date.setDate(date.getDate() + days);
  return isoDay(date);
}

/** 预设范围对应的起止日期，切到「自定义」时用当前正在看的范围作默认值。 */
export function presetSpan(range: string, today: string) {
  if (range === "today") return { start: today, end: today };
  if (range === "7") return { start: addDays(today, -6), end: today };
  if (range === "30") return { start: addDays(today, -29), end: today };
  return { start: `${today.slice(0, 8)}01`, end: today };
}

/** 两个日期排好先后，并且不晚于今天。 */
export function orderSpan(a: string, b: string, today: string) {
  const first = a <= b ? a : b;
  const second = a <= b ? b : a;
  return { start: first > today ? today : first, end: second > today ? today : second };
}

export function spanLabel(start: string, end: string) {
  const s = parseDay(start);
  const e = parseDay(end);
  const left = `${s.getMonth() + 1}月${s.getDate()}日`;
  if (start === end) return left;
  const sameYear = s.getFullYear() === e.getFullYear();
  const right = sameYear ? `${e.getMonth() + 1}月${e.getDate()}日` : `${e.getFullYear()}年${e.getMonth() + 1}月${e.getDate()}日`;
  return `${sameYear ? left : `${s.getFullYear()}年${left}`} – ${right}`;
}

export function daysBetween(start: string, end: string) {
  return Math.round((parseDay(end).getTime() - parseDay(start).getTime()) / 86_400_000) + 1;
}

const weekdays = ["一", "二", "三", "四", "五", "六", "日"];

/** 以周一开头的月历格子；不属于本月的格子为 null。 */
export function monthCells(year: number, month: number) {
  const first = new Date(year, month, 1);
  const lead = (first.getDay() + 6) % 7;
  const count = new Date(year, month + 1, 0).getDate();
  const cells: (string | null)[] = Array.from({ length: lead }, () => null);
  for (let day = 1; day <= count; day += 1) cells.push(isoDay(new Date(year, month, day)));
  while (cells.length % 7 !== 0) cells.push(null);
  return cells;
}

export function DateRangeButton({
  start,
  end,
  today,
  onApply,
}: {
  start: string;
  end: string;
  today: string;
  onApply: (start: string, end: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    function onDown(event: MouseEvent) {
      if (root.current && !root.current.contains(event.target as Node)) setOpen(false);
    }
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [open]);
  return (
    <div className="date-range" ref={root}>
      <button
        type="button"
        className="date-range-pill"
        aria-haspopup="dialog"
        aria-expanded={open}
        onClick={() => setOpen((value) => !value)}
      >
        <IconCalendar />
        <span>{spanLabel(start, end)}</span>
      </button>
      {open ? (
        <DateRangePopover
          start={start}
          end={end}
          today={today}
          onCancel={() => setOpen(false)}
          onApply={(nextStart, nextEnd) => {
            setOpen(false);
            onApply(nextStart, nextEnd);
          }}
        />
      ) : null}
    </div>
  );
}

export function DateRangePopover({
  start,
  end,
  today,
  onCancel,
  onApply,
}: {
  start: string;
  end: string;
  today: string;
  onCancel: () => void;
  onApply: (start: string, end: string) => void;
}) {
  const [draftStart, setDraftStart] = useState(start);
  const [draftEnd, setDraftEnd] = useState<string | null>(end);
  const [hover, setHover] = useState<string | null>(null);
  const [focus, setFocus] = useState(end);
  const [view, setView] = useState(() => {
    const date = parseDay(end);
    return { year: date.getFullYear(), month: date.getMonth() };
  });
  const grid = useRef<HTMLDivElement>(null);

  useEffect(() => {
    grid.current?.querySelector<HTMLButtonElement>(`[data-day="${focus}"]`)?.focus();
  }, [focus, view]);

  const cells = monthCells(view.year, view.month);
  const lastDay = draftEnd ?? hover ?? draftStart;
  const span = orderSpan(draftStart, lastDay, today);
  const atLatest = view.year > parseDay(today).getFullYear() ||
    (view.year === parseDay(today).getFullYear() && view.month >= parseDay(today).getMonth());

  function moveMonth(step: number) {
    setView((current) => {
      const date = new Date(current.year, current.month + step, 1);
      return { year: date.getFullYear(), month: date.getMonth() };
    });
  }

  function pick(day: string) {
    if (day > today) return;
    if (draftEnd === null) {
      const next = orderSpan(draftStart, day, today);
      setDraftStart(next.start);
      setDraftEnd(next.end);
    } else {
      setDraftStart(day);
      setDraftEnd(null);
    }
    setFocus(day);
  }

  function onKey(event: KeyboardEvent<HTMLDivElement>) {
    const steps: Record<string, number> = { ArrowLeft: -1, ArrowRight: 1, ArrowUp: -7, ArrowDown: 7 };
    if (event.key === "Escape") {
      event.preventDefault();
      onCancel();
      return;
    }
    const step = steps[event.key];
    if (step === undefined) return;
    event.preventDefault();
    const next = addDays(focus, step);
    if (next > today) return;
    const date = parseDay(next);
    setView({ year: date.getFullYear(), month: date.getMonth() });
    setFocus(next);
  }

  return (
    <div className="date-popover" role="dialog" aria-label="选择日期范围" onKeyDown={onKey}>
      <header>
        <button type="button" className="date-nav prev" aria-label="上个月" onClick={() => moveMonth(-1)}>
          <IconChevron />
        </button>
        <strong>
          {view.year}年{view.month + 1}月
        </strong>
        <button type="button" className="date-nav" aria-label="下个月" disabled={atLatest} onClick={() => moveMonth(1)}>
          <IconChevron />
        </button>
      </header>
      <div className="date-weekdays" aria-hidden="true">
        {weekdays.map((day) => (
          <span key={day}>{day}</span>
        ))}
      </div>
      <div className="date-grid" ref={grid} role="grid" onMouseLeave={() => setHover(null)}>
        {cells.map((day, index) => {
          if (!day) return <span key={`gap-${index}`} className="date-cell empty" />;
          const future = day > today;
          const edge = day === span.start || day === span.end;
          const inside = day > span.start && day < span.end;
          const classes = ["date-cell"];
          if (inside) classes.push("inside");
          if (edge) classes.push("edge");
          if (day === span.start && span.start !== span.end) classes.push("from");
          if (day === span.end && span.start !== span.end) classes.push("to");
          if (day === today) classes.push("today");
          return (
            <span key={day} className={classes.join(" ")}>
              <button
                type="button"
                data-day={day}
                tabIndex={day === focus ? 0 : -1}
                disabled={future}
                aria-pressed={edge}
                aria-label={spanLabel(day, day)}
                onClick={() => pick(day)}
                onMouseEnter={() => setHover(day)}
              >
                {parseDay(day).getDate()}
              </button>
            </span>
          );
        })}
      </div>
      <footer>
        <span>
          {draftEnd === null ? "再选结束日期" : `${spanLabel(span.start, span.end)} · 共 ${daysBetween(span.start, span.end)} 天`}
        </span>
        <button type="button" className="date-secondary" onClick={onCancel}>
          取消
        </button>
        <button
          type="button"
          className="date-primary"
          disabled={draftEnd === null}
          onClick={() => onApply(span.start, span.end)}
        >
          确定
        </button>
      </footer>
    </div>
  );
}
