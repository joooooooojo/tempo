export type PluginIpc = {
  invokes: {
    greet: (input?: { name?: string }) => { message: string };
  };
  messages: {
    greeted: [payload: { message: string }];
  };
};
