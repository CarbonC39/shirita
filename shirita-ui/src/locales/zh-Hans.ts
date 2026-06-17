import type { MessageSchema } from './en'

const zhHans: MessageSchema = {
  common: {
    save: '保存',
    cancel: '取消',
    delete: '删除',
    duplicate: '复制',
    add: '添加',
    close: '关闭',
    import: '导入',
    export: '导出',
    loading: '加载中…',
    tokensEstimate: '~{count} tokens',
  },
  shell: {
    chats: '对话',
    new: '新建',
    book: '设定集',
    settings: '设置',
  },
  home: {
    empty: '还没有对话。',
    importTitle: '导入对话',
    newChatAria: '新建对话',
    done: '完成',
    reorderDelete: '重排与删除',
    deleteConfirm: '删除此对话及其所有消息？',
  },
  newChat: {
    namePlaceholder: '名称',
    next: '下一步',
    skip: '跳过',
  },
  settings: {
    title: '设置',
    language: '语言',
  },
}

export default zhHans
