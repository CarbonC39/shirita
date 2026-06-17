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
    tokensEstimate: '~{tokens} トークン',
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
  newChat: {
    namePlaceholder: '名前',
    next: '次へ',
    skip: 'スキップ',
  },
  prompt: {
    untitled: '無題',
    subtitle: 'プロンプトテンプレートを選んでツリーを構成します。',
    template: 'テンプレート',
    none: 'なし（空から開始）',
    creating: '作成中…',
    create: '会話を作成',
    deleteContainerConfirm: 'このコンテナと中の {count} 件の項目を削除しますか？',
  },
  chat: {
    back: '戻る',
    title: 'チャット',
  },
  settings: {
    title: '設定',
    language: '言語',
  },
}

export default ja
