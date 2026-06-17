import type { MessageSchema } from './en'

const zhHant: MessageSchema = {
  common: {
    save: '儲存',
    cancel: '取消',
    delete: '刪除',
    duplicate: '複製',
    add: '新增',
    close: '關閉',
    import: '匯入',
    export: '匯出',
    loading: '載入中…',
    tokensEstimate: '~{tokens} tokens',
  },
  shell: {
    chats: '對話',
    new: '新增',
    book: '設定集',
    settings: '設定',
  },
  home: {
    empty: '尚無對話。',
    importTitle: '匯入對話',
    newChatAria: '新增對話',
    done: '完成',
    reorderDelete: '重新排序與刪除',
    deleteConfirm: '刪除此對話及其所有訊息？',
  },
  newChat: {
    namePlaceholder: '名稱',
    next: '下一步',
    skip: '跳過',
  },
  prompt: {
    untitled: '未命名',
    subtitle: '選擇一個提示詞範本並配置節點樹。',
    template: '範本',
    none: '無（從空白開始）',
    creating: '建立中…',
    create: '建立對話',
    deleteContainerConfirm: '刪除此容器及其中的 {count} 個項目？',
  },
  chat: {
    back: '返回',
    title: '對話',
  },
  settings: {
    title: '設定',
    language: '語言',
  },
}

export default zhHant
