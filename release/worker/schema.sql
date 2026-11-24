-- 下载与检查更新记录：不存 IP、不存完整 UA、不存任何可识别个人的标识
CREATE TABLE IF NOT EXISTS events (
  id INTEGER PRIMARY KEY,
  ts INTEGER NOT NULL,          -- 毫秒时间戳
  day TEXT NOT NULL,            -- UTC 日期 YYYY-MM-DD
  kind TEXT NOT NULL,           -- download（官网下载）/ update（应用内下载更新包）/ check（检查更新）
  version TEXT NOT NULL,        -- 下载的版本，或检查更新时的当前版本
  arch TEXT NOT NULL,           -- aarch64 / x86_64 / unknown（官网下载从浏览器分辨不出芯片）
  os TEXT NOT NULL,             -- mac / windows / linux / ios / android / other
  country TEXT NOT NULL,        -- 国家或地区代码（Cloudflare 提供）
  src TEXT NOT NULL             -- 来源：链接上的 from 参数，或来源网站域名
);
CREATE INDEX IF NOT EXISTS events_day_kind ON events (day, kind);
