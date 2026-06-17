import type { MessageSchema } from './en'

const ja: MessageSchema = {
  common: {
    save: '保存',
    cancel: 'キャンセル',
    delete: '削除',
    duplicate: '複製',
    add: '追加',
    close: '閉じる',
    import: 'インポート',
    export: 'エクスポート',
    loading: '読み込み中…',
    tokensEstimate: '~{count} トークン',
  },
  shell: {
    chats: 'チャット',
    new: '新規',
    book: 'ブック',
    settings: '設定',
  },
  home: {
    empty: 'まだ会話がありません。',
    importTitle: '会話をインポート',
    newChatAria: '新規チャット',
    done: '完了',
    reorderDelete: '並べ替え・削除',
    deleteConfirm: 'この会話とすべてのメッセージを削除しますか？',
  },
  settings: {
    title: '設定',
    language: '言語',
  },
}

export default ja
