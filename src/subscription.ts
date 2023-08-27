import {
  OutputSchema as RepoEvent,
  isCommit,
} from "./lexicon/types/com/atproto/sync/subscribeRepos";
import { FirehoseSubscriptionBase, getOpsByType } from "./util/subscription";
import { eueoeo_broadcaster } from "./server";

export class FirehoseSubscription extends FirehoseSubscriptionBase {
  async handleEvent(evt: RepoEvent) {
    if (!isCommit(evt)) return;
    const ops = await getOpsByType(evt);

    const postsToDelete = ops.posts.deletes.map((del) => del.uri);
    const postsToCreate = ops.posts.creates.filter((create) => {
      return create.record.text.trim() == "으어어";
    });

    Promise.allSettled([
      postsToDelete.length > 0
        ? this.db.deleteFrom("post").where("uri", "in", postsToDelete).execute()
        : Promise.resolve(),
      postsToCreate.length > 0
        ? this.db
            .insertInto("post")
            .values(
              postsToCreate.map((create) => {
                // map alf-related posts to a db row
                return {
                  uri: create.uri,
                  cid: create.cid,
                  replyParent: create.record?.reply?.parent.uri ?? null,
                  replyRoot: create.record?.reply?.root.uri ?? null,
                  indexedAt: new Date().toISOString(),
                };
              })
            )
            .onConflict((oc) => oc.doNothing())
            .execute()
        : Promise.resolve(),
      new Promise<void>((resolve) => {
        for (const post of postsToCreate) {
          eueoeo_broadcaster.emit("eueoeo", post.author);
        }

        resolve();
      }),
    ]);
  }
}
