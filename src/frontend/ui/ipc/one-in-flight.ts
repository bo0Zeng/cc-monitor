/**
 * 同一问一次只在飞一问：键由调用方拼（哪一问 · 哪台 · 哪个会话 …，这里不认识）。在飞时再要 ⇒ 不发，只记一笔「回来再补一趟」；
 * 回来以后至多补一趟（期间要了多少次都只补一趟），补之前问调用方给的「还要不要」（不要 ⇒ 不补）。
 * 形同会话事实的数据源（`views/facts-source.ts`）那一份在飞 ＋ 补一趟，只是按键记，给一份对多个会话扇出的那几处共用。
 * 不重试、不设期限（期限在各条读口）。交进来的 `run` 自己收错（各条读口失败照它那一格记），不往外抛；回错与回成一样算回来了。
 */
export class OneInFlight {
  /** 在飞的键 ⇒ 回来再补一趟。 */
  private readonly flying = new Map<string, boolean>();

  /** 要一问：那个键在飞 ⇒ 只记补一趟；不在飞 ⇒ 现在发。`still` 在补之前问（不给 ⇒ 总要）。 */
  ask(key: string, run: () => Promise<void>, still: () => boolean = () => true): void {
    if (this.flying.has(key)) {
      this.flying.set(key, true);
      return;
    }
    this.flying.set(key, false);
    void run().finally(() => {
      const again = this.flying.get(key) === true;
      this.flying.delete(key);
      if (again && still()) this.ask(key, run, still);
    });
  }
}
