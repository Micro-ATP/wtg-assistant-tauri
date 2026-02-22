import './Sidebar.css'

function Sidebar() {
  return (
    <aside className="sidebar">
      <div className="sidebar-header">
        <h2>WTG Assistant</h2>
      </div>
      <nav className="sidebar-nav">
        <ul>
          <li><a href="#home" className="nav-link active">Home</a></li>
          <li><a href="#configure" className="nav-link">Configure</a></li>
          <li><a href="#write" className="nav-link">Write</a></li>
          <li><a href="#benchmark" className="nav-link">Benchmark</a></li>
        </ul>
      </nav>
      <div className="sidebar-footer">
        <p>v2.0.0</p>
      </div>
    </aside>
  )
}

export default Sidebar
