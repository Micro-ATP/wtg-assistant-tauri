import { useTranslation } from 'react-i18next'
import './Tools.css'

function ToolsPage() {
  const { t } = useTranslation()

  const cards = [
    {
      title: t('tools.diskInfo') || '磁盘信息查看',
      description: t('tools.diskInfoDesc') || '显示当前所选磁盘的容量与介质类型。',
    },
    {
      title: t('tools.pathTest') || '路径写入测试',
      description: t('tools.pathTestDesc') || '快速验证目标路径是否具备写入权限。',
    },
    {
      title: t('tools.capacityCalc') || '容量换算',
      description: t('tools.capacityCalcDesc') || '在 GB / GiB 单位之间进行容量换算。',
    },
  ]

  return (
    <div className="tools-page">
      <header className="page-header">
        <h1>{t('tools.title') || '小工具'}</h1>
        <p className="sub">{t('tools.subtitle') || '实用工具集合'}</p>
      </header>

      <section className="tools-panel">
        <div className="tools-grid">
          {cards.map((card) => (
            <div key={card.title} className="tool-card">
              <div className="tool-name">{card.title}</div>
              <div className="tool-desc">{card.description}</div>
            </div>
          ))}
        </div>
        <p className="tools-hint">{t('tools.hint') || '该板块为扩展区，后续会持续新增实用工具。'}</p>
      </section>
    </div>
  )
}

export default ToolsPage
