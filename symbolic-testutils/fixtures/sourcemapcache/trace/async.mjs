import { callsAsyncCallback } from "./shared.mjs";

export async function asyncNamedFn() {
  await asyncArrowFn();
}
const asyncArrowFn = async () => {
  await AsyncKlass.asyncStaticMethod();
};

class AsyncKlass {
  static async asyncStaticMethod() {
    let k = new AsyncKlass();
    await k.asyncClassMethod();
  }
  async asyncClassMethod() {
    await this.#privateAsyncMethod();
  }
  async #privateAsyncMethod() {
    await this.asyncProtoMethod();
  }
}

AsyncKlass.prototype.asyncProtoMethod = async function () {
  await asyncObj.asyncObjectLiteralMethod();
};

let asyncObj = {
  async asyncObjectLiteralMethod() {
    await asyncObj.asyncObjectLiteralAnon();
  },
  asyncObjectLiteralAnon: async () => {
    throw new Error();
  },
};
